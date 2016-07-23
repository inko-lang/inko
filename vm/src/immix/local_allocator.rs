//! Process-local memory allocator
//!
//! The LocalAllocator lives in a Process and is used for allocating memory on a
//! process heap.

use immix::bucket::Bucket;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

/// The number of buckets to use for the young generation.
const YOUNG_BUCKETS: usize = 4;

/// The number of buckets to use for the mature generation.
const MATURE_BUCKETS: usize = 2;

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    global_allocator: RcGlobalAllocator,
    buckets: Vec<Bucket>,
}

/// A tuple containing an allocated object pointer and a boolean that indicates
/// whether or not a GC run should be scheduled.
pub type AllocationResult = (ObjectPointer, bool);

impl LocalAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> LocalAllocator {
        // Prepare the eden bucket
        let mut eden = Bucket::new();
        let (block, _) = global_allocator.request_block();

        eden.add_block(block);

        LocalAllocator {
            global_allocator: global_allocator,
            buckets: vec![eden],
        }
    }

    /// Allocates an object with a prototype.
    pub fn allocate_with_prototype(&mut self,
                                   value: ObjectValue,
                                   proto: ObjectPointer)
                                   -> AllocationResult {
        let object = Object::with_prototype(value, proto);

        self.allocate(object)
    }

    /// Allocates an object without a prototype.
    pub fn allocate_without_prototype(&mut self,
                                      value: ObjectValue)
                                      -> AllocationResult {
        let object = Object::new(value);

        self.allocate(object)
    }

    /// Allocates an empty object without a prototype.
    pub fn allocate_empty(&mut self) -> AllocationResult {
        self.allocate_without_prototype(object_value::none())
    }

    /// Resets and returns all blocks of all buckets to the global allocator.
    pub fn return_blocks(&mut self) {
        for bucket in self.buckets.iter_mut() {
            for mut block in bucket.blocks.drain(0..) {
                block.reset();

                self.global_allocator.add_block(block);
            }
        }
    }

    /// Allocates a prepared Object on the eden heap.
    fn allocate(&mut self, object: Object) -> AllocationResult {
        // Try to allocate into the first available block.
        {
            if let Some(block) = self.eden().first_available_block() {
                return (block.bump_allocate(object), false);
            }
        }

        // We could not allocate into any of the existing blocks, let's request
        // a new one and allocate into it.
        let (block, allocated_new) = self.global_allocator.request_block();
        let mut eden = self.eden();

        eden.add_block(block);

        (eden.bump_allocate(object), allocated_new)
    }

    /// Returns the bucket to use for the eden generation
    fn eden(&mut self) -> &mut Bucket {
        self.buckets.get_mut(0).unwrap()
    }
}
