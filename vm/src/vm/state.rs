//! Virtual Machine States
//!
//! Each virtual machine has its own state. This state includes any scheduled
//! garbage collections, the configuration, the files that have been executed,
//! etc.

use parking_lot::Mutex;
use std::sync::{Arc, RwLock};
use std::collections::HashSet;

use gc::request::Request;

use immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
use immix::permanent_allocator::PermanentAllocator;

use config::Config;
use object_pointer::ObjectPointer;
use pool::Pool;
use pools::Pools;
use process_table::ProcessTable;
use process::RcProcess;

pub type RcState = Arc<State>;

/// The state of a virtual machine.
pub struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// The files that have been executed.
    pub executed_files: RwLock<HashSet<String>>,

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

    /// The top level object.
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

    /// The prototype for file objects.
    pub file_prototype: ObjectPointer,

    /// The prototype for method objects.
    pub method_prototype: ObjectPointer,

    /// The prototype for compiled code objects.
    pub compiled_code_prototype: ObjectPointer,

    /// The prototype for binding objects.
    pub binding_prototype: ObjectPointer,

    /// The singleton "true" object.
    pub true_object: ObjectPointer,

    /// The singleton "false" object.
    pub false_object: ObjectPointer,
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
        let file_proto = perm_alloc.allocate_empty();
        let method_proto = perm_alloc.allocate_empty();
        let cc_proto = perm_alloc.allocate_empty();
        let binding_proto = perm_alloc.allocate_empty();

        let true_obj = perm_alloc.allocate_empty();
        let false_obj = perm_alloc.allocate_empty();

        {
            true_obj.get_mut().set_prototype(true_proto.clone());
            false_obj.get_mut().set_prototype(false_proto.clone());
        }

        let gc_pool = Pool::new(config.gc_threads);

        let process_pools = Pools::new(config.primary_threads,
                                       config.secondary_threads);

        let state = State {
            config: config,
            executed_files: RwLock::new(HashSet::new()),
            process_table: RwLock::new(ProcessTable::new()),
            process_pools: process_pools,
            gc_pool: gc_pool,
            exit_status: Mutex::new(Ok(())),
            permanent_allocator: Mutex::new(perm_alloc),
            global_allocator: global_alloc,
            top_level: top_level,
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            true_prototype: true_proto,
            false_prototype: false_proto,
            file_prototype: file_proto,
            method_prototype: method_proto,
            compiled_code_prototype: cc_proto,
            binding_prototype: binding_proto,
            true_object: true_obj,
            false_object: false_obj,
        };

        Arc::new(state)
    }
}
