//! Process-local memory allocator
//!
//! The LocalAllocator lives in a Process and is used for allocating memory on a
//! process heap.

use std::ops::Drop;

use immix::finalizer_set::FinalizerSet;
use immix::copy_object::CopyObject;
use immix::bucket::Bucket;
use immix::block::BLOCK_SIZE;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

/// Macro that allocates an object in a block and returns a pointer to the
/// allocated object.
macro_rules! allocate {
    ($alloc: expr, $object: ident, $bucket: expr) => ({
        if let Some(block) = $bucket.first_available_block() {
            return (false, block.bump_allocate($object));
        }

        let block = $alloc.global_allocator.request_block();
        let mut block_ref = $bucket.add_block(block);

        (true, block_ref.bump_allocate($object))
    });
}

/// The maximum age of a bucket in the young generation.
pub const YOUNG_MAX_AGE: isize = 3;

/// The maximum number of blocks that can be allocated before a garbage
/// collection of the young generation should be performed.
// pub const YOUNG_BLOCK_ALLOCATION_THRESHOLD: usize = (1 * 1024 * 1024) /
//                                                    BLOCK_SIZE;
pub const YOUNG_BLOCK_ALLOCATION_THRESHOLD: usize = 2;

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

    /// The set containing all young pointers to finalize as some point.
    pub young_finalizer_set: FinalizerSet,

    /// The set containing all mature pointers to finalize at some point.
    pub mature_finalizer_set: FinalizerSet,

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
            young_finalizer_set: FinalizerSet::new(),
            mature_finalizer_set: FinalizerSet::new(),
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

    /// Resets and returns all blocks of all buckets to the global allocator.
    pub fn return_blocks(&mut self) {
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

    /// Allocates an object with a prototype.
    pub fn allocate_with_prototype(&mut self,
                                   value: ObjectValue,
                                   proto: ObjectPointer)
                                   -> ObjectPointer {
        let object = Object::with_prototype(value, proto);

        self.allocate_eden(object)
    }

    /// Allocates an object without a prototype.
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

    /// Allocates an object in the eden space.
    pub fn allocate_eden(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_eden_raw(object);

        if pointer.is_finalizable() {
            self.young_finalizer_set.insert(pointer);
        }

        if new_block {
            self.young_block_allocations += 1;
        }

        pointer
    }

    /// Allocates an object in the mature space.
    pub fn allocate_mature(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = self.allocate_mature_raw(object);

        if pointer.is_finalizable() {
            self.mature_finalizer_set.insert(pointer);
        }

        if new_block {
            self.mature_block_allocations += 1;
        }

        pointer
    }

    /// Allocates an object into a specific bucket.
    pub fn allocate_bucket(&mut self,
                           bucket: &mut Bucket,
                           object: Object)
                           -> (bool, ObjectPointer) {
        allocate!(self, object, bucket)
    }

    /// Increments the age of all buckets in the young generation
    pub fn increment_young_ages(&mut self) {
        for (index, bucket) in self.young_generation.iter_mut().enumerate() {
            if bucket.age == YOUNG_MAX_AGE {
                bucket.reset_age();
                self.eden_index = index;
            } else {
                bucket.increment_age();
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
        allocate!(self, object, self.eden_space_mut())
    }

    fn allocate_mature_raw(&mut self, object: Object) -> (bool, ObjectPointer) {
        allocate!(self, object, self.mature_generation_mut())
    }
}

impl CopyObject for LocalAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate_eden(object)
    }
}

impl Drop for LocalAllocator {
    fn drop(&mut self) {
        self.return_blocks();
    }
}
