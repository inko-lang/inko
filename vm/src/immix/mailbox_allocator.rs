//! Allocator for process mailboxes
//!
//! Each mailbox has its own allocator and its own heap. Incoming messages are
//! copied into this heap. When a message is received its copied from the
//! mailbox heap to the process local heap.

use immix::copy_object::CopyObject;
use immix::bucket::{Bucket, MAILBOX};
use immix::global_allocator::RcGlobalAllocator;
use immix::finalization_list::FinalizationList;
use immix::generation_config::GenerationConfig;

use config::Config;
use object::Object;
use object_pointer::ObjectPointer;

pub struct MailboxAllocator {
    global_allocator: RcGlobalAllocator,

    /// The bucket to allocate objects into.
    pub bucket: Bucket,

    /// The heap configuration.
    pub config: GenerationConfig,
}

impl MailboxAllocator {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> Self {
        let config = GenerationConfig::new(
            config.mailbox_threshold,
            config.mailbox_growth_threshold,
            config.mailbox_growth_factor,
        );

        MailboxAllocator {
            global_allocator: global_allocator,
            bucket: Bucket::with_age(MAILBOX),
            config: config,
        }
    }

    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) =
            self.bucket.allocate(&self.global_allocator, object);

        if new_block {
            self.config.increment_allocations();
        }

        pointer
    }

    /// Prepares a garbage collection cycle, returns true if objects have to be
    /// moved around.
    pub fn prepare_for_collection(&mut self) -> bool {
        self.bucket.prepare_for_collection()
    }

    /// Returns unused blocks to the global allocator.
    pub fn reclaim_blocks(&mut self) -> FinalizationList {
        let (reclaim, finalize) = self.bucket.reclaim_blocks();

        self.global_allocator.add_blocks(reclaim);

        finalize
    }

    pub fn should_collect(&self) -> bool {
        self.config.collect
    }

    pub fn update_block_allocations(&mut self) {
        self.config.block_allocations = self.bucket.number_of_blocks();
    }

    pub fn update_collection_statistics(&mut self) {
        self.config.collect = false;
        self.update_block_allocations();

        if self.config.should_increment() {
            self.config.increment_threshold();
        }
    }
}

impl CopyObject for MailboxAllocator {
    fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
        self.allocate(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;
    use immix::copy_object::CopyObject;
    use config::Config;
    use object::Object;
    use object_value;

    fn mailbox_allocator() -> MailboxAllocator {
        let global = GlobalAllocator::new();

        MailboxAllocator::new(global, &Config::new())
    }

    #[test]
    fn test_allocate() {
        let mut alloc = mailbox_allocator();
        let pointer = alloc.allocate(Object::new(object_value::none()));

        assert!(pointer.is_mailbox());
        assert!(pointer.get().value.is_none());
    }

    #[test]
    fn test_copy_object() {
        let mut mbox_alloc = mailbox_allocator();
        let global_alloc = mbox_alloc.global_allocator.clone();
        let mut local_alloc = LocalAllocator::new(global_alloc, &Config::new());

        let original =
            local_alloc.allocate_without_prototype(object_value::float(5.0));

        let copy = mbox_alloc.copy_object(original);

        assert!(copy.is_mailbox());
        assert!(copy.get().value.is_float());
    }
}
