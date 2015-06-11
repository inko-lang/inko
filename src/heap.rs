use std::sync::{Arc, RwLock};

use object::RcObject;

pub const DEFAULT_CAPACITY: usize = 1024;

/// A mutable, reference counted Heap.
pub type RcHeap = Arc<RwLock<Heap>>;

/// Struct for storing runtime objects.
///
/// Objects stored in a Heap are owned by said heap and use reference counting
/// (using Rc) to allow shared references. Objects should not be shared between
/// threads.
///
pub struct Heap {
    /// Any objects stored on the heap.
    pub objects: Vec<RcObject>
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
            objects: Vec::with_capacity(capacity)
        }
    }

    /// Creates a new reference counted Heap.
    pub fn with_rc() -> RcHeap {
        Arc::new(RwLock::new(Heap::new()))
    }

    /// Stores the given Object on the heap.
    pub fn store(&mut self, object: RcObject) {
        self.objects.push(object);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::{Object, ObjectValue};

    #[test]
    fn test_new() {
        let heap = Heap::new();

        assert_eq!(heap.objects.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn test_with_capacity() {
        let heap = Heap::with_capacity(128);

        assert_eq!(heap.objects.capacity(), 128);
    }

    #[test]
    fn test_with_rc() {
        let heap = Heap::with_rc();

        let heap_ref = heap.read().unwrap();

        assert_eq!(heap_ref.objects.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn test_store() {
        let object   = Object::new(ObjectValue::None);
        let mut heap = Heap::new();

        heap.store(object);

        assert_eq!(heap.objects.len(), 1);
    }
}
