//! Storing of runtime objects on the heap
//!
//! A Heap can be used to store objects that are created during the lifetime of
//! a program. These objects are garbage collected whenever they are no longer
//! in use.

use object::RcObject;

pub const DEFAULT_CAPACITY: usize = 1024;

/// Struct for storing heap objects.
pub struct Heap {
    pub objects: Vec<RcObject>
}

impl Heap {
    pub fn new() -> Heap {
        Heap {
            objects: Vec::with_capacity(DEFAULT_CAPACITY)
        }
    }

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

        assert_eq!(heap.objects.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn test_store() {
        let object = Object::new(1, object_value::none());
        let heap   = Heap::new();

        heap.store(object);

        assert_eq!(heap.objects.len(), 1);
    }
}
