//! Virtual Machine States
//!
//! Each virtual machine has its own state. This state includes any scheduled
//! garbage collections, the configuration, the files that have been parsed,
//! etc.

use parking_lot::Mutex;
use std::sync::{Arc, RwLock};
use std::time;

use gc::request::Request;

use immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
use immix::permanent_allocator::PermanentAllocator;

use config::Config;
use object_pointer::ObjectPointer;
use object_value;
use pool::Pool;
use pools::Pools;
use process_table::ProcessTable;
use process::RcProcess;
use string_pool::StringPool;

pub type RcState = Arc<State>;

/// The state of a virtual machine.
pub struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// Table containing all processes.
    pub process_table: RwLock<ProcessTable<RcProcess>>,

    /// The pool to use for garbage collection.
    pub gc_pool: Pool<Request>,

    /// The process pools to use.
    pub process_pools: Pools,

    /// The exit status of the program.
    pub exit_status: Mutex<Result<(), String>>,

    /// The permanent memory allocator, used for global data.
    pub permanent_allocator: Mutex<Box<PermanentAllocator>>,

    /// The global memory allocator.
    pub global_allocator: RcGlobalAllocator,

    /// Mapping of raw strings and their interned string objects.
    pub string_pool: Mutex<StringPool>,

    /// The start time of the VM (more or less).
    pub start_time: time::Instant,

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

    /// The prototype for the "true" object.
    pub true_prototype: ObjectPointer,

    /// The prototype for the "false" object.
    pub false_prototype: ObjectPointer,

    /// The prototype for method objects.
    pub method_prototype: ObjectPointer,

    /// The prototype for Blocks.
    pub block_prototype: ObjectPointer,

    /// The prototype for binding objects.
    pub binding_prototype: ObjectPointer,

    /// The singleton "true" object.
    pub true_object: ObjectPointer,

    /// The singleton "false" object.
    pub false_object: ObjectPointer,

    /// The prototype for the "nil" object.
    pub nil_prototype: ObjectPointer,

    /// The singleton "nil" object.
    pub nil_object: ObjectPointer,
}

impl State {
    pub fn new(config: Config) -> RcState {
        let global_alloc = GlobalAllocator::new();

        // Boxed since moving around the allocator can break pointers from the
        // blocks back to the allocator's bucket.
        let mut perm_alloc =
            Box::new(PermanentAllocator::new(global_alloc.clone()));

        let top_level = perm_alloc.allocate_empty();
        let integer_proto = perm_alloc.allocate_empty();
        let float_proto = perm_alloc.allocate_empty();
        let string_proto = perm_alloc.allocate_empty();
        let array_proto = perm_alloc.allocate_empty();
        let true_proto = perm_alloc.allocate_empty();
        let false_proto = perm_alloc.allocate_empty();
        let method_proto = perm_alloc.allocate_empty();
        let block_proto = perm_alloc.allocate_empty();
        let binding_proto = perm_alloc.allocate_empty();

        let true_obj = perm_alloc.allocate_empty();
        let false_obj = perm_alloc.allocate_empty();

        let nil_proto = perm_alloc.allocate_empty();
        let nil_obj = perm_alloc.allocate_empty();

        {
            true_obj.get_mut().set_prototype(true_proto.clone());
            false_obj.get_mut().set_prototype(false_proto.clone());
            nil_obj.get_mut().set_prototype(nil_proto.clone());
        }

        let gc_pool = Pool::new(config.gc_threads);

        let process_pools = Pools::new(config.primary_threads,
                                       config.secondary_threads);

        let state = State {
            config: config,
            process_table: RwLock::new(ProcessTable::new()),
            process_pools: process_pools,
            gc_pool: gc_pool,
            exit_status: Mutex::new(Ok(())),
            permanent_allocator: Mutex::new(perm_alloc),
            global_allocator: global_alloc,
            string_pool: Mutex::new(StringPool::new()),
            start_time: time::Instant::now(),
            top_level: top_level,
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            true_prototype: true_proto,
            false_prototype: false_proto,
            method_prototype: method_proto,
            block_prototype: block_proto,
            binding_prototype: binding_proto,
            true_object: true_obj,
            false_object: false_obj,
            nil_prototype: nil_proto,
            nil_object: nil_obj,
        };

        Arc::new(state)
    }

    /// Interns a pointer pointing to a string.
    ///
    /// If the pointer is already interned it's simply returned.
    pub fn intern_pointer(&self,
                          pointer: &ObjectPointer)
                          -> Result<ObjectPointer, String> {
        if pointer.is_permanent() && pointer.get().value.is_string() {
            Ok(*pointer)
        } else {
            Ok(self.intern(pointer.string_value()?))
        }
    }

    /// Interns a string.
    ///
    /// If a string was not yet interned it's allocated in the permanent space.
    pub fn intern(&self, string: &String) -> ObjectPointer {
        let mut pool = self.string_pool.lock();

        if let Some(value) = pool.get(string) {
            return value;
        }

        let ptr = {
            let mut alloc = self.permanent_allocator.lock();
            let value = object_value::string(string.clone());

            alloc.allocate_with_prototype(value, self.string_prototype)
        };

        pool.add(ptr);

        ptr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;

    #[test]
    fn test_intern() {
        let state = State::new(Config::new());
        let string = "number".to_string();

        let ptr1 = state.intern(&string);
        let ptr2 = state.intern(&string);

        assert!(ptr1 == ptr2);

        assert_eq!(ptr1.string_value().unwrap(), ptr2.string_value().unwrap());
    }

    #[test]
    fn test_intern_pointer_with_string() {
        let state = State::new(Config::new());
        let string = state.permanent_allocator
            .lock()
            .allocate_without_prototype(object_value::string("hello"
                .to_string()));

        assert!(state.intern_pointer(&string).unwrap() == string);
    }

    #[test]
    fn test_intern_pointer_without_string() {
        let state = State::new(Config::new());
        let string = state.permanent_allocator
            .lock()
            .allocate_empty();

        assert!(state.intern_pointer(&string).is_err());
    }
}
