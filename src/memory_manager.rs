//! Module for managing memory and prototypes.
//!
//! A MemoryManager can be used to allocate new objects on a heap as well as
//! registering/looking up object prototypes.
//!
//! A MemoryManager struct can be safely shared between threads as any mutable
//! operation uses a read-write lock.

use std::sync::{Arc, RwLock};

use heap::{Heap, RcHeap};
use object::{Object, RcObject};
use object_value;
use thread::RcThread;

/// A reference counted MemoryManager.
pub type RcMemoryManager = Arc<MemoryManager>;

/// Structure for managing memory
pub struct MemoryManager {
    /// The latest available object ID.
    pub object_id: RwLock<usize>,

    /// The top-level object used for storing global constants.
    pub top_level: RcObject,

    /// The young heap, most objects will be allocated here.
    pub young_heap: RcHeap,

    /// The mature heap, used for big objects or those that have outlived
    /// several GC cycles.
    pub mature_heap: RcHeap,

    /// Prototype to use for integer objects.
    pub integer_prototype: RwLock<Option<RcObject>>,

    /// Prototype to use for float objects.
    pub float_prototype: RwLock<Option<RcObject>>,

    /// Prototype to use for string objects.
    pub string_prototype: RwLock<Option<RcObject>>,

    /// Prototype to use for array objects.
    pub array_prototype: RwLock<Option<RcObject>>,

    /// Prototype to use for thread objects.
    pub thread_prototype: RwLock<Option<RcObject>>
}

impl MemoryManager {
    /// Creates a new MemoryManager.
    ///
    /// This also takes care of setting up the top-level object.
    ///
    pub fn new() -> RcMemoryManager {
        let top_level   = Object::new(0, object_value::none());
        let mature_heap = Heap::new();

        write_lock!(top_level).pin();

        write_lock!(mature_heap).store(top_level.clone());

        let manager = MemoryManager {
            object_id: RwLock::new(1),
            top_level: top_level,
            young_heap: Heap::new(),
            mature_heap: mature_heap,
            integer_prototype: RwLock::new(None),
            float_prototype: RwLock::new(None),
            string_prototype: RwLock::new(None),
            array_prototype: RwLock::new(None),
            thread_prototype: RwLock::new(None)
        };

        Arc::new(manager)
    }

    /// Creates and allocates a new RcObject.
    pub fn allocate(&self, value: object_value::ObjectValue, proto: RcObject) -> RcObject {
        let obj = self.new_object(value);

        write_lock!(obj).set_prototype(proto);

        self.allocate_prepared(obj.clone());

        obj
    }

    /// Allocates an exiting RcObject on the heap.
    pub fn allocate_prepared(&self, object: RcObject) {
        write_lock!(self.young_heap).store(object);
    }

    /// Allocates a Thread object based on an existing RcThread.
    pub fn allocate_thread(&self, thread: RcThread) -> RcObject {
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
        read_lock!(self.integer_prototype).clone()
    }

    pub fn float_prototype(&self) -> Option<RcObject> {
        read_lock!(self.float_prototype).clone()
    }

    pub fn string_prototype(&self) -> Option<RcObject> {
        read_lock!(self.string_prototype).clone()
    }

    pub fn array_prototype(&self) -> Option<RcObject> {
        read_lock!(self.array_prototype).clone()
    }

    pub fn thread_prototype(&self) -> Option<RcObject> {
        read_lock!(self.thread_prototype).clone()
    }

    pub fn set_integer_prototype(&self, object: RcObject) {
        let mut proto = write_lock!(self.integer_prototype);

        *proto = Some(object);
    }

    pub fn set_float_prototype(&self, object: RcObject) {
        let mut proto = write_lock!(self.float_prototype);

        *proto = Some(object);
    }

    pub fn set_string_prototype(&self, object: RcObject) {
        let mut proto = write_lock!(self.string_prototype);

        *proto = Some(object);
    }

    pub fn set_array_prototype(&self, object: RcObject) {
        let mut proto = write_lock!(self.array_prototype);

        *proto = Some(object);
    }

    pub fn set_thread_prototype(&self, object: RcObject) {
        let mut proto = write_lock!(self.thread_prototype);

        *proto = Some(object);
    }

    fn new_object_id(&self) -> usize {
        let mut object_id = write_lock!(self.object_id);

        *object_id += 1;

        *object_id
    }

    pub fn new_object(&self, value: object_value::ObjectValue) -> RcObject {
        let obj_id = self.new_object_id();
        let obj    = Object::new(obj_id, value);

        obj
    }
}
