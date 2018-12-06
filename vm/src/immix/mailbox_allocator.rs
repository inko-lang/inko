//! Allocator for process mailboxes
//!
//! Each mailbox has its own allocator and its own heap. Incoming messages are
//! copied into this heap. When a message is received its copied from the
//! mailbox heap to the process local heap.

use config::Config;
use immix::bucket::{Bucket, MAILBOX};
use immix::copy_object::CopyObject;
use immix::generation_config::GenerationConfig;
use immix::global_allocator::RcGlobalAllocator;
use immix::histograms::Histograms;
use object::Object;
use object_pointer::ObjectPointer;
use vm::state::RcState;

pub struct MailboxAllocator {
    global_allocator: RcGlobalAllocator,

    /// The histograms to use for collecting garbage.
    pub histograms: Histograms,

    /// The bucket to allocate objects into.
    pub bucket: Bucket,

    /// The heap configuration.
    pub config: GenerationConfig,
}

impl MailboxAllocator {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> Self {
        MailboxAllocator {
            global_allocator,
            histograms: Histograms::new(),
            bucket: Bucket::with_age(MAILBOX),
            config: GenerationConfig::new(config.mailbox_threshold),
        }
    }

    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        let (new_block, pointer) = unsafe {
            self.bucket
                .allocate_for_mutator(&self.global_allocator, object)
        };

        if new_block {
            self.config.increment_allocations();
        }

        pointer
    }

    /// Prepares a garbage collection cycle, returns true if objects have to be
    /// moved around.
    pub fn prepare_for_collection(&mut self) -> bool {
        self.bucket.prepare_for_collection(&self.histograms)
    }

    /// Returns unused blocks to the global allocator.
    pub fn reclaim_blocks(&mut self, state: &RcState) {
        self.histograms.reset();
        self.bucket.reclaim_blocks(state, &self.histograms);
    }

    pub fn should_collect(&self) -> bool {
        self.config.allocation_threshold_exceeded()
    }

    pub fn update_collection_statistics(&mut self, config: &Config) {
        self.config.block_allocations = 0;

        let blocks = self.bucket.number_of_blocks();

        if self
            .config
            .should_increase_threshold(blocks, config.heap_growth_threshold)
        {
            self.config.increment_threshold(config.heap_growth_factor);
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
    use config::Config;
    use immix::copy_object::CopyObject;
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;
    use object::Object;
    use object_value;

    fn mailbox_allocator() -> MailboxAllocator {
        let global = GlobalAllocator::with_rc();

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
