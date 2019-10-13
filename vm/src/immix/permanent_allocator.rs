//! Permanent Object Allocator
//!
//! This allocator allocates objects that are never garbage collected.
use crate::immix::bucket::{Bucket, PERMANENT};
use crate::immix::copy_object::CopyObject;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::object::Object;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::object_value::ObjectValue;
use std::ops::Drop;

pub struct PermanentAllocator {
    global_allocator: RcGlobalAllocator,

    /// The bucket to allocate objects into.
    bucket: Bucket,
}

impl PermanentAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        PermanentAllocator {
            global_allocator,
            bucket: Bucket::with_age(PERMANENT),
        }
    }

    pub fn allocate_with_prototype(
        &mut self,
        value: ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        self.allocate(Object::with_prototype(value, proto))
    }

    pub fn allocate_without_prototype(
        &mut self,
        value: ObjectValue,
    ) -> ObjectPointer {
        self.allocate(Object::new(value))
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.allocate_without_prototype(object_value::none())
    }

    fn allocate(&mut self, object: Object) -> ObjectPointer {
        let (_, pointer) = unsafe {
            self.bucket
                .allocate_for_mutator(&self.global_allocator, object)
        };

        pointer.mark();
        pointer
    }
}

impl CopyObject for PermanentAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}

impl Drop for PermanentAllocator {
    fn drop(&mut self) {
        for block in self.bucket.blocks.drain() {
            // Dropping the block also finalises it right away.
            drop(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::object_value;

    fn permanent_allocator() -> PermanentAllocator {
        PermanentAllocator::new(GlobalAllocator::with_rc())
    }

    #[test]
    fn test_allocate_with_prototype() {
        let mut alloc = permanent_allocator();
        let proto = alloc.allocate_empty();
        let pointer =
            alloc.allocate_with_prototype(object_value::float(5.0), proto);

        assert!(pointer.get().prototype == proto);
        assert!(pointer.get().value.is_float());
        assert!(pointer.is_permanent());
    }

    #[test]
    fn test_allocate_without_prototype() {
        let mut alloc = permanent_allocator();
        let pointer =
            alloc.allocate_without_prototype(object_value::float(5.0));

        assert!(pointer.get().prototype().is_none());
        assert!(pointer.get().value.is_float());
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
    fn test_allocate_marked() {
        let mut alloc = permanent_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.is_marked());
    }

    #[test]
    fn test_drop() {
        let mut alloc = permanent_allocator();

        alloc.allocate_empty();

        // This is just a smoke test to make sure the dropping doesn't crash in
        // any way.
        drop(alloc);
    }
}
