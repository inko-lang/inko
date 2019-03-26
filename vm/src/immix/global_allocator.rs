//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use arc_without_weak::ArcWithoutWeak;
use crossbeam_queue::SegQueue;
use immix::block::Block;
use immix::block_list::BlockList;

pub type RcGlobalAllocator = ArcWithoutWeak<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    blocks: SegQueue<Box<Block>>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn with_rc() -> RcGlobalAllocator {
        ArcWithoutWeak::new(GlobalAllocator {
            blocks: SegQueue::new(),
        })
    }

    /// Requests a new free block from the pool
    pub fn request_block(&self) -> Box<Block> {
        if let Ok(block) = self.blocks.pop() {
            block
        } else {
            Block::boxed()
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Box<Block>) {
        self.blocks.push(block);
    }

    /// Adds multiple blocks to the global allocator.
    pub fn add_blocks(&self, to_add: &mut BlockList) {
        for block in to_add.drain() {
            self.add_block(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let alloc = GlobalAllocator::with_rc();

        assert!(alloc.blocks.pop().is_err());
    }

    #[test]
    fn test_request_block() {
        let alloc = GlobalAllocator::with_rc();
        let block = alloc.request_block();

        alloc.add_block(block);
        alloc.request_block();

        assert!(alloc.blocks.pop().is_err());
    }

    #[test]
    fn test_add_block() {
        let alloc = GlobalAllocator::with_rc();
        let block = alloc.request_block();

        alloc.add_block(block);

        assert!(alloc.blocks.pop().is_ok());
    }
}
