use object::{Object, RcObject, ObjectValue};

const DEFAULT_CAPACITY: usize = 1024;

/// Struct for storing runtime objects.
///
/// Objects stored in a Heap are owned by said heap and use reference counting
/// (using Rc) to allow shared references. Objects should not be shared between
/// threads.
///
pub struct Heap {
    /// The objects stored on the heap.
    pub members: Vec<RcObject>
}

impl Heap {
    /// Creates a Heap with a default capacity.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///
    pub fn new() -> Heap {
        Heap::with_capacity(DEFAULT_CAPACITY)
    }

    /// Creates a Heap with a custom capacity.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::with_capacity(2048);
    ///
    pub fn with_capacity(capacity: usize) -> Heap {
        Heap { members: Vec::with_capacity(capacity) }
    }

    /// Returns the capacity of the heap.
    pub fn capacity(&self) -> usize {
        self.members.capacity()
    }

    /// Allocates a new object on the heap.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///     let obj  = heap.allocate(ObjectValue::Integer(10));
    ///
    pub fn allocate(&mut self, value: ObjectValue) -> RcObject {
        let object = Object::with_rc(value);

        self.members.push(object.clone());

        object
    }

    /// Allocates a generic object.
    ///
    /// These objects can be used for objects that don't have a specific value
    /// (e.g. a class).
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///     let obj  = heap.allocate_object();
    ///
    pub fn allocate_object(&mut self) -> RcObject {
        self.allocate(ObjectValue::None)
    }

    /// Allocates an integer object.
    ///
    /// These objects automatically have their parent set to the global
    /// "Integer" object.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///     let obj  = heap.allocate_integer(10);
    ///
    pub fn allocate_integer(&mut self, value: isize) -> RcObject {
        self.allocate(ObjectValue::Integer(value))
    }

    /// Allocates a float object.
    ///
    /// These objects automatically have their parent set to the global
    /// "Float" object.
    ///
    /// # Examples
    ///
    ///     let heap = Heap::new();
    ///     let obj  = heap.allocate_float(10.5);
    ///
    pub fn allocate_float(&mut self, value: f64) -> RcObject {
        self.allocate(ObjectValue::Float(value))
    }
}
