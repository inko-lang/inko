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

/// The number of GC cycles an object in the young generation has to survive
/// before being promoted to the next generation.
const YOUNG_MAX_AGE: usize = 4;

/// Structure containing the state of a process-local allocator.
pub struct LocalAllocator {
    global_allocator: RcGlobalAllocator,
    eden: Bucket,
    young: Vec<Bucket>,
    mature: Bucket,
}

/// A tuple containing an allocated object pointer and a boolean that indicates
/// whether or not a GC run should be scheduled.
pub type AllocationResult = (ObjectPointer, bool);

impl LocalAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> LocalAllocator {
        let mut young_generation = Vec::with_capacity(YOUNG_MAX_AGE);

        for _ in 0..YOUNG_MAX_AGE {
            young_generation.push(Bucket::new());
        }

        LocalAllocator {
            global_allocator: global_allocator,
            eden: Bucket::new(),
            young: young_generation,
            mature: Bucket::new(),
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

    /// Allocates a prepared Object on the eden heap.
    fn allocate(&mut self, object: Object) -> AllocationResult {
        // This block is scoped so Rust shuts up about there being multiple
        // mutable references to "self.eden".
        {
            if let Some(block) = self.eden.find_block() {
                return (block.allocate(object), false);
            }
        }

        // No usable block was found, we'll request one from the global
        // allocator and use that block.
        let (block, allocated_new) = self.global_allocator.request_block();

        // It's important we first add the block to the bucket as otherwise we
        // may end up invalidating any pointers created before adding the block.
        let pointer = self.eden.add_block(block).allocate(object);

        (pointer, allocated_new)
    }
}
