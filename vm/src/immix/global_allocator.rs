//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use std::sync::{Arc, Mutex};

use immix::block::{Block, BLOCK_SIZE};

/// The number of blocks to pre-allocate.
pub const PRE_ALLOCATE_BLOCKS: usize = (1 * 1024 * 1024) / BLOCK_SIZE;

pub type RcGlobalAllocator = Arc<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    pub blocks: Mutex<Vec<Box<Block>>>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn new() -> RcGlobalAllocator {
        let mut blocks = Vec::with_capacity(PRE_ALLOCATE_BLOCKS);

        for _ in 0..blocks.capacity() {
            blocks.push(Block::new());
        }

        Arc::new(GlobalAllocator { blocks: Mutex::new(blocks) })
    }

    /// Creates a new global allocator without pre-allocating any blocks.
    pub fn without_preallocated_blocks() -> RcGlobalAllocator {
        Arc::new(GlobalAllocator { blocks: Mutex::new(Vec::new()) })
    }

    /// Requests a new free block from the pool
    pub fn request_block(&self) -> Box<Block> {
        let mut blocks = unlock!(self.blocks);

        if blocks.len() > 0 {
            blocks.pop().unwrap()
        } else {
            Block::new()
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Box<Block>) {
        unlock!(self.blocks).push(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let alloc = GlobalAllocator::new();

        assert_eq!(unlock!(alloc.blocks).len(), PRE_ALLOCATE_BLOCKS);
    }

    #[test]
    fn test_without_preallocated_blocks() {
        let alloc = GlobalAllocator::without_preallocated_blocks();

        assert_eq!(unlock!(alloc.blocks).len(), 0);
    }

    #[test]
    fn test_request_block() {
        let alloc = GlobalAllocator::without_preallocated_blocks();
        let block = alloc.request_block();

        alloc.add_block(block);
        alloc.request_block();

        assert_eq!(unlock!(alloc.blocks).len(), 0);
    }

    #[test]
    fn test_add_block() {
        let alloc = GlobalAllocator::without_preallocated_blocks();
        let block = alloc.request_block();

        alloc.add_block(block);

        assert_eq!(unlock!(alloc.blocks).len(), 1);
    }
}
