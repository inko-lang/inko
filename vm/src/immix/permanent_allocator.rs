//! Permanent Object Allocator
//!
//! This allocator allocates objects that are never garbage collected.

use immix::allocation_result::AllocationResult;
use immix::bucket::Bucket;
use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

macro_rules! make_permanent {
    ($pointer: expr) => ({
        $pointer.get_mut().set_permanent();
        $pointer
    });
}

// Structure containing the state of the permanent allocator.
pub struct PermanentAllocator {
    /// The global allocator from which to request new blocks of memory.
    global_allocator: RcGlobalAllocator,

    /// The bucket containing the permanent objects.
    bucket: Bucket,
}

impl PermanentAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        let mut bucket = Bucket::new();
        let (block, _) = global_allocator.request_block();

        bucket.add_block(block);

        PermanentAllocator {
            global_allocator: global_allocator,
            bucket: bucket,
        }
    }

    /// Allocates an object with a prototype.
    pub fn allocate_with_prototype(&mut self,
                                   value: ObjectValue,
                                   proto: ObjectPointer)
                                   -> ObjectPointer {
        let object = Object::with_prototype(value, proto);

        self.allocate(object)
    }

    /// Allocates an object without a prototype.
    pub fn allocate_without_prototype(&mut self,
                                      value: ObjectValue)
                                      -> ObjectPointer {
        let object = Object::new(value);

        self.allocate(object)
    }

    /// Allocates an empty object without a prototype.
    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(object_value::none())
    }

    /// Allocates a prepared Object on the heap.
    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        {
            if let Some(block) = self.bucket.first_available_block() {
                let pointer = block.bump_allocate(object);

                return make_permanent!(pointer);
            }
        }

        let (block, _) = self.global_allocator.request_block();

        self.bucket.add_block(block);

        let pointer = self.bucket.bump_allocate(object);

        make_permanent!(pointer)
    }
}

impl CopyObject for PermanentAllocator {
    fn allocate_copy(&mut self, object: Object) -> AllocationResult {
        (self.allocate(object), false)
    }
}
