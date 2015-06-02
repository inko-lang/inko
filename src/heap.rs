use class::{Class, RcClass};
use object::RcObject;

const DEFAULT_CAPACITY: usize = 1024;

/// Struct for storing runtime objects.
///
/// Objects stored in a Heap are owned by said heap and use reference counting
/// (using Rc) to allow shared references. Objects should not be shared between
/// threads.
///
pub struct Heap {
    /// Any objects stored on the heap.
    pub objects: Vec<RcObject>,

    /// Any classes stored on the heap. These are usually pinned and may be
    /// larger than an Object, hence they are stored separately. Unpinned
    /// classes are only to be GC'd when there are no instances of it on the
    /// heap. This is only the case for anonymous classes.
    pub classes: Vec<RcClass>
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
        Heap {
            objects: Vec::with_capacity(capacity),
            classes: Vec::with_capacity(capacity)
        }
    }

    /// Stores the given Object on the heap.
    pub fn store_object(&mut self, object: RcObject) {
        self.objects.push(object);
    }

    /// Stores the given Class on the heap.
    pub fn store_class(&mut self, klass: RcClass) {
        self.classes.push(klass);
    }

    /// Allocates a new pinned, named Class.
    pub fn allocate_pinned_class(&mut self, name: String) -> RcClass {
        let klass = Class::with_pinned_rc(Some(name));

        self.store_class(klass.clone());

        klass
    }
}
