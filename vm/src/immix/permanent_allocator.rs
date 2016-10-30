//! Permanent Object Allocator
//!
//! This allocator allocates objects that are never garbage collected.

use std::ops::Drop;

use immix::bucket::Bucket;
use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_value;
use object_value::ObjectValue;
use object_pointer::ObjectPointer;

pub struct PermanentAllocator {
    global_allocator: RcGlobalAllocator,
    bucket: Bucket,
}

impl PermanentAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        PermanentAllocator {
            global_allocator: global_allocator,
            bucket: Bucket::new(),
        }
    }

    pub fn allocate_with_prototype(&mut self,
                                   value: ObjectValue,
                                   proto: ObjectPointer)
                                   -> ObjectPointer {
        self.allocate(Object::with_prototype(value, proto))
    }

    pub fn allocate_without_prototype(&mut self,
                                      value: ObjectValue)
                                      -> ObjectPointer {
        self.allocate(Object::new(value))
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(object_value::none())
    }

    fn allocate(&mut self, object: Object) -> ObjectPointer {
        let pointer = self.allocate_raw(object);

        pointer.get_mut().set_permanent();

        pointer
    }

    fn allocate_raw(&mut self, object: Object) -> ObjectPointer {
        {
            if let Some(block) = self.bucket.first_available_block() {
                return block.bump_allocate(object);
            }
        }

        let block = self.global_allocator.request_block();

        self.bucket.add_block(block);

        self.bucket.bump_allocate(object)
    }
}

impl CopyObject for PermanentAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}

impl Drop for PermanentAllocator {
    fn drop(&mut self) {
        for mut block in self.bucket.blocks.drain(0..) {
            block.reset();
            self.global_allocator.add_block(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::global_allocator::GlobalAllocator;
    use object_value;

    fn permanent_allocator() -> PermanentAllocator {
        PermanentAllocator::new(GlobalAllocator::without_preallocated_blocks())
    }

    #[test]
    fn test_allocate_with_prototype() {
        let mut alloc = permanent_allocator();
        let proto = alloc.allocate_empty();
        let pointer =
            alloc.allocate_with_prototype(object_value::integer(5), proto);

        assert!(pointer.get().prototype == proto);
        assert!(pointer.get().value.is_integer());
        assert!(pointer.is_permanent());
    }

    #[test]
    fn test_allocate_without_prototype() {
        let mut alloc = permanent_allocator();
        let pointer = alloc.allocate_without_prototype(object_value::integer(5));

        assert!(pointer.get().prototype().is_none());
        assert!(pointer.get().value.is_integer());
        assert!(pointer.is_permanent());
    }

    #[test]
    fn test_allocate_empty() {
        let mut alloc = permanent_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.get().value.is_none());
        assert!(pointer.get().prototype().is_none());
        assert!(pointer.is_permanent());
    }

    #[test]
    fn test_drop() {
        let mut alloc = permanent_allocator();
        let global_alloc = alloc.global_allocator.clone();

        alloc.allocate_empty();

        drop(alloc);

        assert_eq!(unlock!(global_alloc.blocks).len(), 1);
    }
}
