//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use parking_lot::Mutex;
use std::sync::Arc;

use immix::block::Block;
use immix::block_list::BlockList;

pub type RcGlobalAllocator = Arc<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    pub blocks: Mutex<BlockList>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn new() -> RcGlobalAllocator {
        Arc::new(GlobalAllocator {
            blocks: Mutex::new(BlockList::new()),
        })
    }

    /// Requests a new free block from the pool
    pub fn request_block(&self) -> Box<Block> {
        let mut blocks = self.blocks.lock();

        if let Some(block) = blocks.pop_front() {
            block
        } else {
            Block::new()
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Box<Block>) {
        self.blocks.lock().push_back(block);
    }

    /// Adds multiple blocks to the global allocator.
    pub fn add_blocks(&self, to_add: &mut BlockList) {
        let mut blocks = self.blocks.lock();

        blocks.append(to_add);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let alloc = GlobalAllocator::new();

        assert_eq!(alloc.blocks.lock().len(), 0);
    }

    #[test]
    fn test_request_block() {
        let alloc = GlobalAllocator::new();
        let block = alloc.request_block();

        alloc.add_block(block);
        alloc.request_block();

        assert_eq!(alloc.blocks.lock().len(), 0);
    }

    #[test]
    fn test_add_block() {
        let alloc = GlobalAllocator::new();
        let block = alloc.request_block();

        alloc.add_block(block);

        assert_eq!(alloc.blocks.lock().len(), 1);
    }
}
