//! Virtual Machine States
//!
//! Each virtual machine has its own state. This state includes any scheduled
//! garbage collections, the configuration, the files that have been parsed,
//! etc.
use config::Config;
use deref_pointer::DerefPointer;
use gc::request::Request;
use immix::block::Block;
use immix::copy_object::CopyObject;
use immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
use immix::permanent_allocator::PermanentAllocator;
use immutable_string::ImmutableString;
use num_bigint::BigInt;
use object_pointer::ObjectPointer;
use object_value;
use parking_lot::Mutex;
use pool::Pool;
use pools::Pools;
use process::RcProcess;
use process_table::ProcessTable;
use std::panic::RefUnwindSafe;
use std::sync::{Arc, RwLock};
use std::time;
use string_pool::StringPool;
use suspension_list::SuspensionList;

/// A reference counted State.
pub type RcState = Arc<State>;

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

        if ptr.is_finalizable() {
            ptr.mark_for_finalization();
        }

        pool.add(ptr);

        ptr
    }};
}

/// The state of a virtual machine.
pub struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// Table containing all processes.
    pub process_table: RwLock<ProcessTable<RcProcess>>,

    /// The pool to use for garbage collection.
    pub gc_pool: Pool<Request>,

    /// The pool to use for finalizing objects.
    pub finalizer_pool: Pool<DerefPointer<Block>>,

    /// The process pools to use.
    pub process_pools: Pools,

    /// The permanent memory allocator, used for global data.
    pub permanent_allocator: Mutex<Box<PermanentAllocator>>,

    /// The global memory allocator.
    pub global_allocator: RcGlobalAllocator,

    /// Mapping of raw strings and their interned string objects.
    pub string_pool: Mutex<StringPool>,

    /// The start time of the VM (more or less).
    pub start_time: time::Instant,

    /// The list of suspended processes.
    pub suspension_list: SuspensionList,

    /// The exit status to use when the VM terminates.
    pub exit_status: Mutex<i32>,

    /// The prototype of the base object, used as the prototype for all other
    /// prototypes.
    pub object_prototype: ObjectPointer,

    /// The global top-level object.
    pub top_level: ObjectPointer,

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

    /// The singleton "nil" object.
    pub nil_object: ObjectPointer,

    /// The prototype for read-only files.
    pub read_only_file_prototype: ObjectPointer,

    /// The prototype for write-only files.
    pub write_only_file_prototype: ObjectPointer,

    /// The prototype for read-write files.
    pub read_write_file_prototype: ObjectPointer,

    /// The prototype for byte arrays.
    pub byte_array_prototype: ObjectPointer,

    /// The prototype for hashers.
    pub hasher_prototype: ObjectPointer,

    /// The prototype to use for dynamically opened libraries.
    pub library_prototype: ObjectPointer,

    /// The prototype to use for C functions.
    pub function_prototype: ObjectPointer,

    /// The prototype to use for pointers to C variables.
    pub pointer_prototype: ObjectPointer,

    /// The commandline arguments passed to an Inko program.
    pub arguments: Vec<ObjectPointer>,

    /// The default panic handler for all processes.
    ///
    /// This field defaults to a null pointer. Reading and writing this field
    /// should be done using atomic operations.
    pub default_panic_handler: ObjectPointer,
}

impl RefUnwindSafe for State {}

impl State {
    pub fn new(config: Config, arguments: &[String]) -> RcState {
        let global_alloc = GlobalAllocator::new();

        // Boxed since moving around the allocator can break pointers from the
        // blocks back to the allocator's bucket.
        let mut perm_alloc =
            Box::new(PermanentAllocator::new(global_alloc.clone()));

        let object_proto = perm_alloc.allocate_empty();
        let top_level = perm_alloc.allocate_empty();
        let integer_proto = perm_alloc.allocate_empty();
        let float_proto = perm_alloc.allocate_empty();
        let string_proto = perm_alloc.allocate_empty();
        let array_proto = perm_alloc.allocate_empty();
        let block_proto = perm_alloc.allocate_empty();

        let boolean_proto = perm_alloc.allocate_empty();
        let true_obj = perm_alloc.allocate_empty();
        let false_obj = perm_alloc.allocate_empty();
        let nil_obj = perm_alloc.allocate_empty();
        let read_only_file_prototype = perm_alloc.allocate_empty();
        let write_only_file_prototype = perm_alloc.allocate_empty();
        let read_write_file_prototype = perm_alloc.allocate_empty();
        let byte_array_prototype = perm_alloc.allocate_empty();
        let hasher_prototype = perm_alloc.allocate_empty();
        let library_prototype = perm_alloc.allocate_empty();
        let function_prototype = perm_alloc.allocate_empty();
        let pointer_prototype = perm_alloc.allocate_empty();

        {
            top_level.set_prototype(object_proto);
            integer_proto.set_prototype(object_proto);
            float_proto.set_prototype(object_proto);
            string_proto.set_prototype(object_proto);
            array_proto.set_prototype(object_proto);
            block_proto.set_prototype(object_proto);

            boolean_proto.set_prototype(object_proto);
            nil_obj.set_prototype(object_proto);
            true_obj.set_prototype(boolean_proto);
            false_obj.set_prototype(boolean_proto);

            read_only_file_prototype.set_prototype(object_proto);
            write_only_file_prototype.set_prototype(object_proto);
            read_write_file_prototype.set_prototype(object_proto);
            byte_array_prototype.set_prototype(object_proto);
            hasher_prototype.set_prototype(object_proto);
            library_prototype.set_prototype(object_proto);
            function_prototype.set_prototype(object_proto);
            pointer_prototype.set_prototype(object_proto);
        }

        let gc_pool = Pool::new(config.gc_threads, Some("GC".to_string()));

        let finalizer_pool =
            Pool::new(config.finalizer_threads, Some("finalizer".to_string()));

        let process_pools =
            Pools::new(config.primary_threads, config.secondary_threads);

        let mut state = State {
            config,
            process_table: RwLock::new(ProcessTable::new()),
            process_pools,
            gc_pool,
            finalizer_pool,
            permanent_allocator: Mutex::new(perm_alloc),
            global_allocator: global_alloc,
            string_pool: Mutex::new(StringPool::new()),
            start_time: time::Instant::now(),
            exit_status: Mutex::new(0),
            suspension_list: SuspensionList::new(),
            top_level,
            object_prototype: object_proto,
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            block_prototype: block_proto,
            boolean_prototype: boolean_proto,
            true_object: true_obj,
            false_object: false_obj,
            nil_object: nil_obj,
            arguments: Vec::with_capacity(arguments.len()),
            default_panic_handler: ObjectPointer::null(),
            read_only_file_prototype,
            write_only_file_prototype,
            read_write_file_prototype,
            byte_array_prototype,
            hasher_prototype,
            library_prototype,
            function_prototype,
            pointer_prototype,
        };

        for argument in arguments {
            let pointer = state.intern_string(argument.clone());

            state.arguments.push(pointer);
        }

        Arc::new(state)
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
    #[cfg_attr(feature = "cargo-clippy", allow(ptr_arg))]
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

    pub fn set_exit_status(&self, new_status: i32) {
        *self.exit_status.lock() = new_status;
    }

    pub fn current_exit_status(&self) -> i32 {
        *self.exit_status.lock()
    }

    pub fn set_default_panic_handler(
        &self,
        handler: ObjectPointer,
    ) -> Result<ObjectPointer, String> {
        if handler.block_value()?.captures_from.is_some() {
            return Err("default panic handlers can not capture any variables"
                .to_string());
        }

        let handler_to_use = if handler.is_permanent() {
            handler
        } else {
            self.permanent_allocator.lock().copy_object(handler)
        };

        self.default_panic_handler
            .raw
            .atomic_store(handler_to_use.raw.raw);

        Ok(handler_to_use)
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
    use config::Config;

    #[test]
    fn test_intern() {
        let state = State::new(Config::new(), &[]);
        let string = ImmutableString::from("number".to_string());

        let ptr1 = state.intern(&string);
        let ptr2 = state.intern(&string);

        assert!(ptr1 == ptr2);

        assert_eq!(ptr1.string_value().unwrap(), ptr2.string_value().unwrap());
    }

    #[test]
    fn test_intern_pointer_with_string() {
        let state = State::new(Config::new(), &[]);
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
        let state = State::new(Config::new(), &[]);
        let string = state.permanent_allocator.lock().allocate_empty();

        assert!(state.intern_pointer(string).is_err());
    }

    #[test]
    fn test_allocate_permanent_float() {
        let state = State::new(Config::new(), &[]);
        let float = state.allocate_permanent_float(10.5);

        assert_eq!(float.float_value().unwrap(), 10.5);
    }
}
