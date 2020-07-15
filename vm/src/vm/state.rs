//! Virtual Machine States
//!
//! Each virtual machine has its own state. This state includes any scheduled
//! garbage collections, the configuration, the files that have been parsed,
//! etc.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use crate::gc::coordinator::Pool as GcPool;
use crate::immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
use crate::immix::permanent_allocator::PermanentAllocator;
use crate::immutable_string::ImmutableString;
use crate::network_poller::NetworkPoller;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::scheduler::process_scheduler::ProcessScheduler;
use crate::scheduler::timeout_worker::TimeoutWorker;
use crate::string_pool::StringPool;
use num_bigint::BigInt;
use parking_lot::Mutex;
use std::panic::RefUnwindSafe;
use std::time;

/// A reference counted State.
pub type RcState = ArcWithoutWeak<State>;

macro_rules! intern_string {
    ($state:expr, $lookup:expr, $store:expr) => {{
        let mut pool = $state.string_pool.lock();

        if let Some(value) = pool.get($lookup) {
            return value;
        }

        let ptr = {
            let mut alloc = $state.permanent_allocator.lock();
            let value = object_value::interned_string($store);

            alloc.allocate_with_prototype(value, $state.string_prototype)
        };

        pool.add(ptr);

        ptr
    }};
}

/// The state of a virtual machine.
pub struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// The scheduler to use for executing Inko processes.
    pub scheduler: ProcessScheduler,

    /// The pool to use for garbage collection.
    pub gc_pool: GcPool,

    /// The permanent memory allocator, used for global data.
    pub permanent_allocator: Mutex<Box<PermanentAllocator>>,

    /// The global memory allocator.
    pub global_allocator: RcGlobalAllocator,

    /// Mapping of raw strings and their interned string objects.
    pub string_pool: Mutex<StringPool>,

    /// The start time of the VM (more or less).
    pub start_time: time::Instant,

    /// The exit status to use when the VM terminates.
    pub exit_status: Mutex<i32>,

    pub timeout_worker: TimeoutWorker,

    /// The prototype of the base object, used as the prototype for all other
    /// prototypes.
    pub object_prototype: ObjectPointer,

    /// The prototype for integer objects.
    pub integer_prototype: ObjectPointer,

    /// The prototype for float objects.
    pub float_prototype: ObjectPointer,

    /// The prototype for string objects.
    pub string_prototype: ObjectPointer,

    /// The prototype for array objects.
    pub array_prototype: ObjectPointer,

    /// The prototype for Blocks.
    pub block_prototype: ObjectPointer,

    /// The prototype for booleans.
    pub boolean_prototype: ObjectPointer,

    /// The singleton "true" object.
    pub true_object: ObjectPointer,

    /// The singleton "false" object.
    pub false_object: ObjectPointer,

    /// The prototype for the "nil" object.
    pub nil_prototype: ObjectPointer,

    /// The singleton "nil" object.
    pub nil_object: ObjectPointer,

    /// The prototype for byte arrays.
    pub byte_array_prototype: ObjectPointer,

    /// The prototype to use for modules.
    pub module_prototype: ObjectPointer,

    /// The commandline arguments passed to an Inko program.
    pub arguments: Vec<ObjectPointer>,

    /// The default panic handler for all processes.
    ///
    /// This field defaults to a null pointer. Reading and writing this field
    /// should be done using atomic operations.
    pub default_panic_handler: ObjectPointer,

    /// The system polling mechanism to use for polling non-blocking sockets.
    pub network_poller: NetworkPoller,
}

impl RefUnwindSafe for State {}

impl State {
    pub fn with_rc(config: Config, arguments: &[String]) -> RcState {
        let global_alloc = GlobalAllocator::with_rc();

        // Boxed since moving around the allocator can break pointers from the
        // blocks back to the allocator's bucket.
        let mut perm_alloc =
            Box::new(PermanentAllocator::new(global_alloc.clone()));

        let object_proto = perm_alloc.allocate_empty();
        let integer_proto = perm_alloc.allocate_empty();
        let float_proto = perm_alloc.allocate_empty();
        let string_proto = perm_alloc.allocate_empty();
        let array_proto = perm_alloc.allocate_empty();
        let block_proto = perm_alloc.allocate_empty();

        let boolean_proto = perm_alloc.allocate_empty();
        let true_obj = perm_alloc.allocate_empty();
        let false_obj = perm_alloc.allocate_empty();
        let nil_proto = perm_alloc.allocate_empty();
        let nil_obj = perm_alloc.allocate_empty();
        let byte_array_proto = perm_alloc.allocate_empty();
        let module_proto = perm_alloc.allocate_empty();

        {
            integer_proto.set_prototype(object_proto);
            float_proto.set_prototype(object_proto);
            string_proto.set_prototype(object_proto);
            array_proto.set_prototype(object_proto);
            block_proto.set_prototype(object_proto);
            nil_proto.set_prototype(object_proto);
            boolean_proto.set_prototype(object_proto);

            nil_obj.set_prototype(nil_proto);
            true_obj.set_prototype(boolean_proto);
            false_obj.set_prototype(boolean_proto);

            byte_array_proto.set_prototype(object_proto);
            module_proto.set_prototype(object_proto)
        }

        let gc_pool = GcPool::new(config.gc_threads);

        let mut state = State {
            scheduler: ProcessScheduler::new(
                config.primary_threads,
                config.blocking_threads,
            ),
            config,
            gc_pool,
            permanent_allocator: Mutex::new(perm_alloc),
            global_allocator: global_alloc,
            string_pool: Mutex::new(StringPool::new()),
            start_time: time::Instant::now(),
            exit_status: Mutex::new(0),
            timeout_worker: TimeoutWorker::new(),
            object_prototype: object_proto,
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            block_prototype: block_proto,
            boolean_prototype: boolean_proto,
            true_object: true_obj,
            false_object: false_obj,
            nil_prototype: nil_proto,
            nil_object: nil_obj,
            arguments: Vec::with_capacity(arguments.len()),
            default_panic_handler: ObjectPointer::null(),
            byte_array_prototype: byte_array_proto,
            module_prototype: module_proto,
            network_poller: NetworkPoller::new(),
        };

        for argument in arguments {
            let pointer = state.intern_string(argument.clone());

            state.arguments.push(pointer);
        }

        ArcWithoutWeak::new(state)
    }

    /// Interns a pointer pointing to a string.
    ///
    /// If the pointer is already interned it's simply returned.
    pub fn intern_pointer(
        &self,
        pointer: ObjectPointer,
    ) -> Result<ObjectPointer, ImmutableString> {
        if pointer.is_interned_string() {
            Ok(pointer)
        } else {
            Ok(self.intern(pointer.string_value()?))
        }
    }

    /// Interns a borrowed String.
    ///
    /// If a string was not yet interned it's allocated in the permanent space.
    pub fn intern(&self, string: &ImmutableString) -> ObjectPointer {
        intern_string!(self, string, string.clone())
    }

    /// Interns an owned String.
    pub fn intern_string(&self, string: String) -> ObjectPointer {
        let to_intern = ImmutableString::from(string);

        intern_string!(self, &to_intern, to_intern)
    }

    pub fn allocate_permanent_float(&self, float: f64) -> ObjectPointer {
        let mut alloc = self.permanent_allocator.lock();
        let value = object_value::float(float);

        alloc.allocate_with_prototype(value, self.float_prototype)
    }

    pub fn allocate_permanent_integer(&self, integer: i64) -> ObjectPointer {
        let mut alloc = self.permanent_allocator.lock();
        let value = object_value::integer(integer);

        alloc.allocate_with_prototype(value, self.integer_prototype)
    }

    pub fn allocate_permanent_bigint(&self, bigint: BigInt) -> ObjectPointer {
        let mut alloc = self.permanent_allocator.lock();
        let value = object_value::bigint(bigint);

        alloc.allocate_with_prototype(value, self.integer_prototype)
    }

    pub fn terminate(&self, status: i32) {
        self.set_exit_status(status);
        self.scheduler.terminate();
        self.gc_pool.terminate();
        self.timeout_worker.terminate();
    }

    pub fn set_exit_status(&self, new_status: i32) {
        *self.exit_status.lock() = new_status;
    }

    pub fn current_exit_status(&self) -> i32 {
        *self.exit_status.lock()
    }

    pub fn default_panic_handler(&self) -> Option<ObjectPointer> {
        let handler = self.default_panic_handler.atomic_load();

        if handler.is_null() {
            None
        } else {
            Some(handler)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_intern() {
        let state = State::with_rc(Config::new(), &[]);
        let string = ImmutableString::from("number".to_string());

        let ptr1 = state.intern(&string);
        let ptr2 = state.intern(&string);

        assert!(ptr1 == ptr2);

        assert_eq!(ptr1.string_value().unwrap(), ptr2.string_value().unwrap());
    }

    #[test]
    fn test_intern_pointer_with_string() {
        let state = State::with_rc(Config::new(), &[]);
        let string = state
            .permanent_allocator
            .lock()
            .allocate_without_prototype(object_value::interned_string(
                ImmutableString::from("hello".to_string()),
            ));

        assert!(state.intern_pointer(string).unwrap() == string);
    }

    #[test]
    fn test_intern_pointer_without_string() {
        let state = State::with_rc(Config::new(), &[]);
        let string = state.permanent_allocator.lock().allocate_empty();

        assert!(state.intern_pointer(string).is_err());
    }

    #[test]
    fn test_allocate_permanent_float() {
        let state = State::with_rc(Config::new(), &[]);
        let float = state.allocate_permanent_float(10.5);

        assert_eq!(float.float_value().unwrap(), 10.5);
    }
}
