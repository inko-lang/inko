//! Allocator for process mailboxes
//!
//! Each mailbox has its own allocator and its own heap. Incoming messages are
//! copied into this heap. When a message is received its copied from the
//! mailbox heap to the process local heap.

use std::ops::Drop;

use immix::copy_object::CopyObject;
use immix::bucket::Bucket;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_pointer::ObjectPointer;

pub struct MailboxAllocator {
    global_allocator: RcGlobalAllocator,
    bucket: Bucket,
}

impl MailboxAllocator {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        MailboxAllocator {
            global_allocator: global_allocator,
            bucket: Bucket::new(),
        }
    }

    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        let pointer = self.allocate_raw(object);

        pointer.get_mut().set_mailbox();

        pointer
    }

    fn allocate_raw(&mut self, object: Object) -> ObjectPointer {
        let (_, pointer) = self.bucket.allocate(&self.global_allocator, object);

        pointer
    }
}

impl CopyObject for MailboxAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}

impl Drop for MailboxAllocator {
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
    use immix::local_allocator::LocalAllocator;
    use immix::copy_object::CopyObject;
    use object::Object;
    use object_value;

    fn mailbox_allocator() -> MailboxAllocator {
        let global = GlobalAllocator::without_preallocated_blocks();

        MailboxAllocator::new(global)
    }

    #[test]
    fn test_allocate() {
        let mut alloc = mailbox_allocator();
        let pointer = alloc.allocate(Object::new(object_value::none()));

        assert!(pointer.get().generation().is_mailbox());
        assert!(pointer.get().value.is_none());
    }

    #[test]
    fn test_copy_object() {
        let mut mbox_alloc = mailbox_allocator();
        let global_alloc = mbox_alloc.global_allocator.clone();
        let mut local_alloc = LocalAllocator::new(global_alloc);

        let original =
            local_alloc.allocate_without_prototype(object_value::integer(5));

        let copy = mbox_alloc.copy_object(original);

        assert!(copy.get().generation().is_mailbox());
        assert!(copy.get().value.is_integer());
    }

    #[test]
    fn test_drop() {
        let mut alloc = mailbox_allocator();
        let global_alloc = alloc.global_allocator.clone();

        alloc.allocate(Object::new(object_value::none()));

        drop(alloc);

        assert_eq!(unlock!(global_alloc.blocks).len(), 1);
    }
}
