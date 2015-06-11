use std::sync::RwLock;

use heap::{Heap, RcHeap};
use object::{Object, ObjectValue, RcObject};

/// Structure for managing memory
///
/// This struct and its implementation mainly act as wrappers around the various
/// available heaps and garbage collectors. This makes it easier to trigger GC
/// runs, allocate objects and perform other operations without dumping all of
/// this in the VirtualMachine struct.
///
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
    pub fn new() -> MemoryManager {
        let top_level   = Object::new(ObjectValue::None);
        let mature_heap = Heap::new();

        top_level.write().unwrap().pin();

        mature_heap.write().unwrap().store(top_level.clone());

        MemoryManager {
            top_level: top_level,
            young_heap: Heap::new(),
            mature_heap: mature_heap,
            integer_prototype: RwLock::new(None),
            float_prototype: RwLock::new(None),
            string_prototype: RwLock::new(None),
            array_prototype: RwLock::new(None),
            thread_prototype: RwLock::new(None)
        }
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
