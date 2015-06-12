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
    // The top-level object used for storing global constants.
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
        let top_level   = Object::new(ObjectValue::None);
        let mature_heap = Heap::new();

        top_level.write().unwrap().pin();

        mature_heap.write().unwrap().store(top_level.clone());

        let manager = MemoryManager {
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
        let obj = Object::new(value);

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
            let obj = Object::new(ObjectValue::Thread(thread));

            self.allocate_prepared(obj.clone());

            obj
        };

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
}
