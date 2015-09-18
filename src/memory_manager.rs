//! Module for managing memory and prototypes.
//!
//! A MemoryManager can be used to allocate new objects on a heap as well as
//! registering/looking up object prototypes.
//!
//! A MemoryManager struct can be safely shared between threads as any mutable
//! operation uses a read-write lock.

use std::sync::{Arc, RwLock};

use heap::Heap;
use object::{Object, RcObject};
use object_value;
use thread::RcThread;

pub type RcMemoryManager = Arc<RwLock<MemoryManager>>;

/// Structure for managing memory
pub struct MemoryManager {
    /// The latest available object ID.
    pub object_id: usize,

    /// The top-level object used for storing global constants.
    pub top_level: RcObject,

    /// The young heap, most objects will be allocated here.
    pub young_heap: Heap,

    /// The mature heap, used for big objects or those that have outlived
    /// several GC cycles.
    pub mature_heap: Heap,

    pub integer_prototype: Option<RcObject>,
    pub float_prototype: Option<RcObject>,
    pub string_prototype: Option<RcObject>,
    pub array_prototype: Option<RcObject>,
    pub thread_prototype: Option<RcObject>
}

impl MemoryManager {
    pub fn new() -> RcMemoryManager {
        let top_level       = Object::new(0, object_value::none());
        let mut mature_heap = Heap::new();

        write_lock!(top_level).pin();

        mature_heap.store(top_level.clone());

        let manager = MemoryManager {
            object_id: 1,
            top_level: top_level,
            young_heap: Heap::new(),
            mature_heap: Heap::new(),
            integer_prototype: None,
            float_prototype: None,
            string_prototype: None,
            array_prototype: None,
            thread_prototype: None
        };

        Arc::new(RwLock::new(manager))
    }

    /// Creates and allocates a new RcObject.
    pub fn allocate(&mut self, value: object_value::ObjectValue, proto: RcObject) -> RcObject {
        let obj = self.new_object(value);

        write_lock!(obj).set_prototype(proto);

        self.allocate_prepared(obj.clone());

        obj
    }

    /// Allocates an exiting RcObject on the heap.
    pub fn allocate_prepared(&mut self, object: RcObject) {
        self.young_heap.store(object);
    }

    /// Allocates a Thread object based on an existing RcThread.
    pub fn allocate_thread(&mut self, thread: RcThread) -> RcObject {
        let proto = self.thread_prototype();

        let thread_obj = if proto.is_some() {
            self.allocate(object_value::thread(thread), proto.unwrap().clone())
        }
        else {
            let obj = self.new_object(object_value::thread(thread));

            self.allocate_prepared(obj.clone());

            obj
        };

        // Prevent the thread from being GC'd if there are no references to it.
        write_lock!(thread_obj).pin();

        thread_obj
    }

    pub fn integer_prototype(&self) -> Option<RcObject> {
        self.integer_prototype.clone()
    }

    pub fn float_prototype(&self) -> Option<RcObject> {
        self.float_prototype.clone()
    }

    pub fn string_prototype(&self) -> Option<RcObject> {
        self.string_prototype.clone()
    }

    pub fn array_prototype(&self) -> Option<RcObject> {
        self.array_prototype.clone()
    }

    pub fn thread_prototype(&self) -> Option<RcObject> {
        self.thread_prototype.clone()
    }

    pub fn set_integer_prototype(&mut self, object: RcObject) {
        self.integer_prototype = Some(object);
    }

    pub fn set_float_prototype(&mut self, object: RcObject) {
        self.float_prototype = Some(object);
    }

    pub fn set_string_prototype(&mut self, object: RcObject) {
        self.string_prototype = Some(object);
    }

    pub fn set_array_prototype(&mut self, object: RcObject) {
        self.array_prototype = Some(object);
    }

    pub fn set_thread_prototype(&mut self, object: RcObject) {
        self.thread_prototype = Some(object);
    }

    fn new_object_id(&mut self) -> usize {
        self.object_id += 1;

        self.object_id
    }

    pub fn new_object(&mut self, value: object_value::ObjectValue) -> RcObject {
        let obj_id = self.new_object_id();
        let obj    = Object::new(obj_id, value);

        obj
    }
}
