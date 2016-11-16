//! Process-local memory allocator
//!
//! The LocalAllocator lives in a Process and is used for allocating memory on a
//! process heap.

use std::ops::Drop;

use immix::copy_object::CopyObject;
use immix::bucket::Bucket;
use immix::block::BLOCK_SIZE;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

/// The maximum age of a bucket in the young generation.
pub const YOUNG_MAX_AGE: isize = 3;

/// The maximum number of blocks that can be allocated before a garbage
/// collection of the young generation should be performed.
pub const YOUNG_BLOCK_ALLOCATION_THRESHOLD: usize = (1 * 1024 * 1024) /
                                                    BLOCK_SIZE;

/// The maximum number of blocks that can be allocated before a garbage
/// collection of the mature generation should be performed.
pub const MATURE_BLOCK_ALLOCATION_THRESHOLD: usize = (2 * 1024 * 1024) /
                                                     BLOCK_SIZE;

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    /// The global allocated from which to request blocks of memory and return
    /// unused blocks to.
    pub global_allocator: RcGlobalAllocator,

    /// The buckets to use for the eden and young survivor spaces.
    pub young_generation: [Bucket; 4],

    /// The position of the eden bucket in the young generation.
    pub eden_index: usize,

    /// The bucket to use for the mature generation.
    pub mature_generation: Bucket,

    /// The number of blocks allocated for the mature generation since the last
    /// garbage collection cycle.
    pub young_block_allocations: usize,

    /// The number of blocks allocated for the mature generation since the last
    /// garbage collection cycle.
    pub mature_block_allocations: usize,
}

impl LocalAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> LocalAllocator {
        LocalAllocator {
            global_allocator: global_allocator,
            young_generation: [Bucket::with_age(0),
                               Bucket::with_age(-1),
                               Bucket::with_age(-2),
                               Bucket::with_age(-3)],
            eden_index: 0,
            mature_generation: Bucket::new(),
            young_block_allocations: 0,
            mature_block_allocations: 0,
        }
    }

    pub fn global_allocator(&self) -> RcGlobalAllocator {
        self.global_allocator.clone()
    }

    pub fn eden_space_mut(&mut self) -> &mut Bucket {
        &mut self.young_generation[self.eden_index]
    }

    pub fn mature_generation_mut(&mut self) -> &mut Bucket {
        &mut self.mature_generation
    }

    /// Returns unused blocks to the global allocator.
    pub fn reclaim_blocks(&mut self, mature: bool) {
        for bucket in self.young_generation.iter_mut() {
            for block in bucket.reclaim_blocks() {
                self.global_allocator.add_block(block);
            }
        }

        if mature {
            for block in self.mature_generation.reclaim_blocks() {
                self.global_allocator.add_block(block);
            }
        }
    }

    pub fn allocate_with_prototype(&mut self,
                                   value: ObjectValue,
                                   proto: ObjectPointer)
                                   -> ObjectPointer {
        let object = Object::with_prototype(value, proto);

        self.allocate_eden(object)
    }

    pub fn allocate_without_prototype(&mut self,
                                      value: ObjectValue)
                                      -> ObjectPointer {
        let object = Object::new(value);

        self.allocate_eden(object)
    }

    /// Allocates an empty object without a prototype.
    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(object_value::none())
    }

    pub fn allocate_eden(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_eden_raw(object);

        if new_block {
            self.young_block_allocations += 1;
        }

        pointer
    }

    pub fn allocate_mature(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_mature_raw(object);

        pointer.get_mut().set_mature();

        if new_block {
            self.mature_block_allocations += 1;
        }

        pointer
    }

    /// Increments the age of all buckets in the young generation
    pub fn increment_young_ages(&mut self) {
        for (index, bucket) in self.young_generation.iter_mut().enumerate() {
            if bucket.age == YOUNG_MAX_AGE {
                bucket.reset_age();
            } else {
                bucket.increment_age();
            }

            if bucket.age == 0 {
                self.eden_index = index;
            }
        }
    }

    /// Returns true if the number of allocated blocks for the young generation
    /// exceeds its threshold.
    pub fn young_block_allocation_threshold_exceeded(&self) -> bool {
        self.young_block_allocations >= YOUNG_BLOCK_ALLOCATION_THRESHOLD
    }

    /// Returns true if the number of allocated blocks for the mature generation
    /// exceeds its threshold.
    pub fn mature_block_allocation_threshold_exceeded(&self) -> bool {
        self.mature_block_allocations >= MATURE_BLOCK_ALLOCATION_THRESHOLD
    }

    // Because Rust's borrow checker is sometimes dumb as a brick when it comes
    // to scoping mutable borrows we have to use two layers of indirection (a
    // function and a macro) to make the following allocation functions work.
    //
    // This can probably be removed once scoping of mutable borrows is handled
    // in a better way: https://github.com/rust-lang/rfcs/issues/811

    fn allocate_eden_raw(&mut self, object: Object) -> (bool, ObjectPointer) {
        self.young_generation[self.eden_index]
            .allocate(&self.global_allocator, object)
    }

    fn allocate_mature_raw(&mut self, object: Object) -> (bool, ObjectPointer) {
        self.mature_generation.allocate(&self.global_allocator, object)
    }
}

impl CopyObject for LocalAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate_eden(object)
    }
}

impl Drop for LocalAllocator {
    fn drop(&mut self) {
        for bucket in self.young_generation.iter_mut() {
            for mut block in bucket.blocks.drain(0..) {
                block.reset();
                self.global_allocator.add_block(block);
            }
        }

        for mut block in self.mature_generation.blocks.drain(0..) {
            block.reset();
            self.global_allocator.add_block(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::global_allocator::GlobalAllocator;
    use immix::copy_object::CopyObject;
    use object::Object;
    use object_value;

    fn local_allocator() -> LocalAllocator {
        LocalAllocator::new(GlobalAllocator::without_preallocated_blocks())
    }

    #[test]
    fn test_new() {
        let alloc = local_allocator();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, -1);
        assert_eq!(alloc.young_generation[2].age, -2);
        assert_eq!(alloc.young_generation[3].age, -3);

        assert_eq!(alloc.eden_index, 0);

        assert_eq!(alloc.young_block_allocations, 0);
        assert_eq!(alloc.mature_block_allocations, 0);
    }

    #[test]
    fn test_global_allocator() {
        let alloc = local_allocator();
        let global_alloc = alloc.global_allocator();

        assert_eq!(unlock!(global_alloc.blocks).len(), 0);
    }

    #[test]
    fn test_eden_space_mut() {
        let mut alloc = local_allocator();

        assert_eq!(alloc.eden_space_mut().age, 0);
    }

    #[test]
    fn test_mature_generation_mut() {
        let mut alloc = local_allocator();

        assert_eq!(alloc.mature_generation_mut().age, 0);
    }

    #[test]
    fn reclaim_blocks() {
        let mut alloc = local_allocator();

        let block1 = alloc.global_allocator.request_block();
        let block2 = alloc.global_allocator.request_block();

        alloc.eden_space_mut().add_block(block1);
        alloc.mature_generation_mut().add_block(block2);

        alloc.reclaim_blocks(false);

        assert_eq!(alloc.eden_space_mut().blocks.len(), 0);
        assert_eq!(alloc.mature_generation_mut().blocks.len(), 1);

        alloc.reclaim_blocks(true);

        assert_eq!(alloc.mature_generation_mut().blocks.len(), 0);
    }

    #[test]
    fn test_allocate_with_prototype() {
        let mut alloc = local_allocator();
        let proto = alloc.allocate_empty();
        let pointer =
            alloc.allocate_with_prototype(object_value::integer(5), proto);

        assert!(pointer.get().prototype == proto);
        assert!(pointer.get().value.is_integer());
    }

    #[test]
    fn test_allocate_without_prototype() {
        let mut alloc = local_allocator();
        let pointer = alloc.allocate_without_prototype(object_value::integer(5));

        assert!(pointer.get().prototype().is_none());
        assert!(pointer.get().value.is_integer());
    }

    #[test]
    fn test_allocate_empty() {
        let mut alloc = local_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.get().value.is_none());
        assert!(pointer.get().prototype().is_none());
    }

    #[test]
    fn test_allocate_eden() {
        let mut alloc = local_allocator();
        let ptr1 = alloc.allocate_eden(Object::new(object_value::none()));

        let ptr2 = alloc.allocate_eden(
            Object::new(object_value::string("a".to_string())));

        assert_eq!(alloc.young_block_allocations, 1);

        assert!(ptr1.is_young());
        assert!(ptr2.is_young());
    }

    #[test]
    fn test_allocate_mature() {
        let mut alloc = local_allocator();
        let ptr1 = alloc.allocate_mature(Object::new(object_value::none()));

        let ptr2 = alloc.allocate_mature(
            Object::new(object_value::string("a".to_string())));

        assert_eq!(alloc.mature_block_allocations, 1);

        assert!(ptr1.is_mature());
        assert!(ptr2.is_mature());
    }

    #[test]
    fn test_increment_young_ages() {
        let mut alloc = local_allocator();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, -1);
        assert_eq!(alloc.young_generation[2].age, -2);
        assert_eq!(alloc.young_generation[3].age, -3);
        assert_eq!(alloc.eden_index, 0);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 1);
        assert_eq!(alloc.young_generation[1].age, 0);
        assert_eq!(alloc.young_generation[2].age, -1);
        assert_eq!(alloc.young_generation[3].age, -2);
        assert_eq!(alloc.eden_index, 1);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 2);
        assert_eq!(alloc.young_generation[1].age, 1);
        assert_eq!(alloc.young_generation[2].age, 0);
        assert_eq!(alloc.young_generation[3].age, -1);
        assert_eq!(alloc.eden_index, 2);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 3);
        assert_eq!(alloc.young_generation[1].age, 2);
        assert_eq!(alloc.young_generation[2].age, 1);
        assert_eq!(alloc.young_generation[3].age, 0);
        assert_eq!(alloc.eden_index, 3);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 0);
        assert_eq!(alloc.young_generation[1].age, 3);
        assert_eq!(alloc.young_generation[2].age, 2);
        assert_eq!(alloc.young_generation[3].age, 1);
        assert_eq!(alloc.eden_index, 0);

        alloc.increment_young_ages();

        assert_eq!(alloc.young_generation[0].age, 1);
        assert_eq!(alloc.young_generation[1].age, 0);
        assert_eq!(alloc.young_generation[2].age, 3);
        assert_eq!(alloc.young_generation[3].age, 2);
        assert_eq!(alloc.eden_index, 1);
    }

    #[test]
    fn test_young_block_allocation_threshold_exceeded() {
        let mut alloc = local_allocator();

        assert_eq!(alloc.young_block_allocation_threshold_exceeded(), false);

        alloc.young_block_allocations = YOUNG_BLOCK_ALLOCATION_THRESHOLD + 1;

        assert!(alloc.young_block_allocation_threshold_exceeded());
    }

    #[test]
    fn test_mature_block_allocation_threshold_exceeded() {
        let mut alloc = local_allocator();

        assert_eq!(alloc.mature_block_allocation_threshold_exceeded(), false);

        alloc.mature_block_allocations = MATURE_BLOCK_ALLOCATION_THRESHOLD + 1;

        assert!(alloc.mature_block_allocation_threshold_exceeded());
    }

    #[test]
    fn test_copy_object() {
        let mut alloc = local_allocator();
        let pointer = alloc.allocate_without_prototype(object_value::integer(5));
        let copy = alloc.copy_object(pointer);

        assert!(copy.is_young());
        assert!(copy.get().value.is_integer());
    }

    #[test]
    fn test_drop() {
        let mut alloc = local_allocator();
        let global_alloc = alloc.global_allocator();

        let block1 = global_alloc.request_block();
        let block2 = global_alloc.request_block();

        alloc.eden_space_mut().add_block(block1);
        alloc.mature_generation_mut().add_block(block2);

        drop(alloc);

        assert_eq!(unlock!(global_alloc.blocks).len(), 2);
    }
}
