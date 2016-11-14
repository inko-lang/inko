//! Data structures for tracking native resources to finalize.
//!
//! A FinalizerSet can be used to track pointers to objects containing native
//! resources (e.g. a Rust string). By tracking these pointers explicitly the
//! garbage collector can release resources whenever an object becomes
//! unreachable.
use std::collections::HashSet;
use object_pointer::ObjectPointer;

pub struct FinalizerSet {
    /// Pointers to objects that may need to be finalized.
    pub pointers: HashSet<ObjectPointer>,
}

impl FinalizerSet {
    pub fn new() -> Self {
        FinalizerSet { pointers: HashSet::new() }
    }

    /// Inserts a pointer into the set.
    pub fn insert(&mut self, pointer: ObjectPointer) {
        self.pointers.insert(pointer);
    }

    /// Removes a pointer from the set.
    pub fn remove(&mut self, pointer: &ObjectPointer) {
        self.pointers.remove(pointer);
    }

    /// Finalizes unreachable objects.
    pub fn finalize(&mut self) {
        let mut retain = HashSet::new();

        for pointer in self.pointers.drain() {
            if pointer.is_marked() {
                retain.insert(pointer);
            } else {
                pointer.finalize();
            }
        }

        self.pointers = retain;
    }

    /// Finalizes all objects, reachable or not.
    pub fn finalize_all(&mut self) {
        for pointer in self.pointers.drain() {
            let mut object = pointer.get_mut();

            object.deallocate_pointers();

            drop(object);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::local_allocator::LocalAllocator;
    use immix::global_allocator::GlobalAllocator;

    fn local_allocator() -> LocalAllocator {
        LocalAllocator::new(GlobalAllocator::without_preallocated_blocks())
    }

    #[test]
    fn test_insert() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);

        assert_eq!(set.pointers.contains(&pointer), true);
    }

    #[test]
    fn test_remove() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.remove(&pointer);

        assert_eq!(set.pointers.contains(&pointer), false);
    }

    #[test]
    fn test_finalize_empty() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.finalize();

        assert_eq!(set.pointers.len(), 0);
    }

    #[test]
    fn test_finalize_with_header() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer1 = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        pointer1.get_mut().add_method("a".to_string(), pointer2);

        set.insert(pointer1);
        set.finalize();

        assert_eq!(set.pointers.len(), 0);
    }

    #[test]
    fn test_finalize_all() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();

        set.insert(pointer);
        set.finalize_all();

        assert_eq!(set.pointers.len(), 0);
    }
}
