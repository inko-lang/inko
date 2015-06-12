//! Module for managing memory and prototypes.
//!
//! A MemoryManager can be used to allocate new objects on a heap as well as
//! registering/looking up object prototypes.
//!
//! A MemoryManager struct can be safely shared between threads as any mutable
//! operation uses a read-write lock.

use std::sync::{Arc, RwLock};

use heap::{Heap, RcHeap};
use object::{Object, ObjectValue, RcObject};
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

    /// Prototype to use for thread objets.
    pub thread_prototype: RwLock<Option<RcObject>>
}

impl MemoryManager {
    /// Creates a new MemoryManager.
    ///
    /// This also takes care of setting up the top-level object.
    ///
    pub fn new() -> RcMemoryManager {
        let top_level   = Object::new(0, ObjectValue::None);
        let mature_heap = Heap::new();

        top_level.write().unwrap().pin();

        mature_heap.write().unwrap().store(top_level.clone());

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
    pub fn allocate(&self, value: ObjectValue, proto: RcObject) -> RcObject {
        let obj = self.new_object(value);

        obj.write().unwrap().set_prototype(proto);

        self.allocate_prepared(obj.clone());

        obj
    }

    /// Allocates an exiting RcObject on the heap.
    pub fn allocate_prepared(&self, object: RcObject) {
        self.young_heap.write().unwrap().store(object);
    }

    /// Allocates a Thread object based on an existing RcThread.
    pub fn allocate_thread(&self, thread: RcThread) -> RcObject {
        let proto = self.thread_prototype();

        let thread_obj = if proto.is_some() {
            self.allocate(ObjectValue::Thread(thread), proto.unwrap().clone())
        }
        else {
            let obj = self.new_object(ObjectValue::Thread(thread));

            self.allocate_prepared(obj.clone());

            obj
        };

        // Prevent the thread from being GC'd if there are no references to it.
        thread_obj.write().unwrap().pin();

        thread_obj
    }

    /// Returns the integer prototype.
    pub fn integer_prototype(&self) -> Option<RcObject> {
        self.integer_prototype.read().unwrap().clone()
    }

    /// Returns the float prototype.
    pub fn float_prototype(&self) -> Option<RcObject> {
        self.float_prototype.read().unwrap().clone()
    }

    /// Returns the string prototype.
    pub fn string_prototype(&self) -> Option<RcObject> {
        self.string_prototype.read().unwrap().clone()
    }

    /// Returns the array prototype.
    pub fn array_prototype(&self) -> Option<RcObject> {
        self.array_prototype.read().unwrap().clone()
    }

    /// Returns the thread prototype.
    pub fn thread_prototype(&self) -> Option<RcObject> {
        self.thread_prototype.read().unwrap().clone()
    }

    /// Sets the integer prototype.
    pub fn set_integer_prototype(&self, object: RcObject) {
        let mut proto = self.integer_prototype.write().unwrap();

        *proto = Some(object);
    }

    /// Sets the float prototype.
    pub fn set_float_prototype(&self, object: RcObject) {
        let mut proto = self.float_prototype.write().unwrap();

        *proto = Some(object);
    }

    /// Sets the string prototype.
    pub fn set_string_prototype(&self, object: RcObject) {
        let mut proto = self.string_prototype.write().unwrap();

        *proto = Some(object);
    }

    /// Sets the array prototype.
    pub fn set_array_prototype(&self, object: RcObject) {
        let mut proto = self.array_prototype.write().unwrap();

        *proto = Some(object);
    }

    /// Sets the thread prototype.
    pub fn set_thread_prototype(&self, object: RcObject) {
        let mut proto = self.thread_prototype.write().unwrap();

        *proto = Some(object);
    }

    /// Returns a new object ID.
    fn new_object_id(&self) -> usize {
        let mut object_id = self.object_id.write().unwrap();

        *object_id += 1;

        *object_id
    }

    /// Returns a new Object with an object ID.
    pub fn new_object(&self, value: ObjectValue) -> RcObject {
        let obj_id = self.new_object_id();
        let obj    = Object::new(obj_id, value);

        obj
    }
}
