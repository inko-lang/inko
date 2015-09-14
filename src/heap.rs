//! Storing of runtime objects on the heap
//!
//! A Heap can be used to store objects that are created during the lifetime of
//! a program. These objects are garbage collected whenever they are no longer
//! in use.

use std::sync::{Arc, RwLock};

use object::RcObject;

pub const DEFAULT_CAPACITY: usize = 1024;

/// A mutable, reference counted Heap.
pub type RcHeap = Arc<RwLock<Heap>>;

/// Struct for storing heap objects.
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
    pub fn new() -> RcHeap {
        let heap = Heap {
            objects: Vec::with_capacity(DEFAULT_CAPACITY)
        };

        Arc::new(RwLock::new(heap))
    }

    /// Stores the given Object on the heap.
    pub fn store(&mut self, object: RcObject) {
        self.objects.push(object);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::Object;
    use object_value;

    #[test]
    fn test_new() {
        let heap = Heap::new();

        assert_eq!(heap.read().unwrap().objects.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn test_store() {
        let object = Object::new(1, object_value::none());
        let heap   = Heap::new();

        heap.write().unwrap().store(object);

        assert_eq!(heap.read().unwrap().objects.len(), 1);
    }
}
