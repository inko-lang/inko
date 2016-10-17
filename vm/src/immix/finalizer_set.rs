//! Data structures for tracking native resources to finalize.
//!
//! A FinalizerSet can be used to track pointers to objects containing native
//! resources (e.g. a Rust string). By tracking these pointers explicitly the
//! garbage collector can release resources whenever an object becomes
//! unreachable.
//!
//! Finalizer sets are made up out of two sets: a "from" set and a "to" set.
//! These sets are used in a manner similar to semispace collectors. When
//! pointers are tracked they are inserted into the "from" set. Whenever the
//! collector marks an object as reachable the pointer is moved to the "to" set.
//! At the end of a collection cycle all pointers that remain in the "from" set
//! are finalized. Once this is done the two sets are swapped with each other.

use std::ops::Drop;

use std::mem;
use std::collections::HashSet;
use object_pointer::ObjectPointer;

pub const RESET_LIMIT: usize = 8;

pub struct FinalizerSet {
    /// The set used to initially store pointers in.
    pub from: HashSet<ObjectPointer>,

    /// The set used to store pointers to reachable objects after marking.
    pub to: HashSet<ObjectPointer>,

    /// The number of times the sets have been swapped.
    pub swaps: usize,
}

impl FinalizerSet {
    pub fn new() -> Self {
        FinalizerSet {
            from: HashSet::new(),
            to: HashSet::new(),
            swaps: 0,
        }
    }

    /// Inserts a pointer into the set.
    pub fn insert(&mut self, pointer: ObjectPointer) {
        self.from.insert(pointer);
    }

    /// Removes a pointer from the set.
    pub fn remove(&mut self, pointer: &ObjectPointer) {
        self.from.remove(pointer);
        self.to.remove(pointer);
    }

    /// Moves a pointer from the "from" to the "to" set so the object is not
    /// finalized.
    pub fn retain(&mut self, pointer_ref: &ObjectPointer) {
        if let Some(pointer) = self.from.take(pointer_ref) {
            self.to.insert(pointer);
        }
    }

    /// Swaps the "from" and "to" sets.
    pub fn swap_sets(&mut self) {
        self.from.clear();

        if self.swaps >= RESET_LIMIT {
            self.from.shrink_to_fit();
            self.to.shrink_to_fit();

            self.swaps = 0;
        } else {
            self.swaps += 1;
        }

        mem::swap(&mut self.from, &mut self.to);
    }

    /// Finalizes all unreachable objects.
    pub fn finalize(&mut self) {
        self.finalize_pointers(&self.from);
        self.swap_sets();
    }

    fn finalize_pointers(&self, pointers: &HashSet<ObjectPointer>) {
        for pointer in pointers.iter() {
            let mut object = pointer.get_mut();

            object.deallocate_pointers();

            drop(object);
        }
    }
}

impl Drop for FinalizerSet {
    fn drop(&mut self) {
        self.finalize_pointers(&self.from);
        self.finalize_pointers(&self.to);
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

        assert_eq!(set.from.contains(&pointer), true);
        assert_eq!(set.to.contains(&pointer), false);
    }

    #[test]
    fn test_remove() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.retain(&pointer);

        // Make sure the pointer is in both from/to
        set.insert(pointer);

        set.remove(&pointer);

        assert_eq!(set.from.contains(&pointer), false);
        assert_eq!(set.to.contains(&pointer), false);
    }

    #[test]
    fn test_retain() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.retain(&pointer);

        assert_eq!(set.from.contains(&pointer), false);
        assert_eq!(set.to.contains(&pointer), true);
    }

    #[test]
    fn test_swap_sets() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.retain(&pointer);
        set.swap_sets();

        assert_eq!(set.from.contains(&pointer), true);
        assert_eq!(set.to.contains(&pointer), false);
    }

    #[test]
    fn test_swap_sets_resize() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.retain(&pointer);
        set.insert(pointer);

        for _ in 0..(RESET_LIMIT + 1) {
            set.swap_sets();
        }

        assert_eq!(set.swaps, 0);
        assert_eq!(set.from.capacity(), 0);
        assert_eq!(set.to.capacity(), 0);
    }

    #[test]
    fn test_finalize_empty() {
        let mut set = FinalizerSet::new();
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        set.insert(pointer);
        set.finalize();
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
    }
}
