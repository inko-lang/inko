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

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    /// The global allocated from which to request blocks of memory and return
    /// unused blocks to.
    global_allocator: RcGlobalAllocator,

    /// Buckets for the eden space and the survivor spaces. The buckets for the
    /// survivor spaces are only allocated when needed.
    ///
    /// The order of buckets is as follows:
    ///
    ///     0: eden space
    ///     1: survivor space 1
    ///     2: survivor space 2
    ///     3: survivor space 3
    young_generation: Vec<Bucket>,

    /// The bucket to use for the mature generation. This bucket is only
    /// allocated when needed.
    mature_generation: Option<Bucket>,
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
            young_generation: vec![eden],
            mature_generation: None,
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
        let mut blocks = Vec::new();

        for bucket in self.young_generation.iter_mut() {
            for block in bucket.blocks.drain(0..) {
                blocks.push(block);
            }
        }

        if let Some(mature) = self.mature_generation.as_mut() {
            for block in mature.blocks.drain(0..) {
                blocks.push(block);
            }
        }

        for mut block in blocks {
            block.reset();
            self.global_allocator.add_block(block);
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
        self.young_generation.get_mut(0).unwrap()
    }
}
