//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use std::sync::{Arc, Mutex};

use immix::block::{Block, BLOCK_SIZE};

/// The number of blocks to pre-allocate.
const PRE_ALLOCATE_BLOCKS: usize = (1 * 1024 * 1024) / BLOCK_SIZE;

pub type RcGlobalAllocator = Arc<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    blocks: Mutex<Vec<Box<Block>>>,
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
        self.compact();
    }

    /// Compacts the list of blocks if needed.
    pub fn compact(&self) {
        let mut blocks = unlock!(self.blocks);

        if blocks.capacity() / blocks.len() >= 4 {
            blocks.shrink_to_fit();
        }
    }
}
