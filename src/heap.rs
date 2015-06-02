use class::Class;
use object::{ObjectType, RcObjectType};

const DEFAULT_CAPACITY: usize = 1024;

/// Struct for storing runtime objects.
///
/// Objects stored in a Heap are owned by said heap and use reference counting
/// (using Rc) to allow shared references. Objects should not be shared between
/// threads.
///
pub struct Heap {
    /// The objects stored on the heap.
    pub members: Vec<RcObjectType>
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

    /// Stores the given object on the heap.
    pub fn store(&mut self, object: RcObjectType) {
        self.members.push(object);
    }

    /// Allocates a new pinned, named Class.
    pub fn allocate_pinned_class(&mut self, name: String) -> RcObjectType {
        let klass = ObjectType::rc_class(Class::with_pinned_rc(Some(name)));

        self.store(klass.clone());

        klass
    }
}
