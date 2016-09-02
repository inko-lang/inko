//! Allocator for process mailboxes
//!
//! Each mailbox has its own allocator and its own heap. These heaps are garbage
//! collected just like the eden space. Unlike the local or permanent allocator
//! this allocator doesn't support the various allocation methods (e.g.
//! allocating objects with prototypes), instead objects are meant to be copied
//! to/from the heap managed by the mailbox allocator.

use immix::copy_object::CopyObject;
use immix::bucket::Bucket;
use immix::global_allocator::RcGlobalAllocator;

use object::Object;
use object_pointer::ObjectPointer;

/// Structure containing the state of the mailbox allocator.
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

    /// Allocates a prepared Object on the heap.
    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
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

impl CopyObject for MailboxAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}
