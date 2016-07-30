//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use std::sync::{Arc, Mutex};

use immix::block::{Block, BLOCK_SIZE};

/// The number of bytes to pre-allocate for blocks.
const PRE_ALLOCATE: usize = 1 * 1024 * 1024;

pub type RcGlobalAllocator = Arc<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    blocks: Mutex<Vec<Block>>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn new() -> RcGlobalAllocator {
        let capacity = PRE_ALLOCATE / BLOCK_SIZE;
        let mut blocks = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            // blocks.push(Block::new());
        }

        Arc::new(GlobalAllocator { blocks: Mutex::new(blocks) })
    }

    /// Requests a new free block from the pool
    ///
    /// The return value is a tuple containing a block and a boolean that
    /// indicates if a new block had to be allocated. The boolean can be used to
    /// determine if a process should trigger a garbage collection.
    pub fn request_block(&self) -> (Block, bool) {
        let mut blocks = unlock!(self.blocks);

        if blocks.len() > 0 {
            (blocks.pop().unwrap(), false)
        } else {
            (Block::new(), true)
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Block) {
        let block = unlock!(self.blocks).push(block);

        self.compact();

        block
    }

    /// Compacts the list of blocks if needed.
    pub fn compact(&self) {
        let mut blocks = unlock!(self.blocks);

        if blocks.capacity() / blocks.len() >= 4 {
            blocks.shrink_to_fit();
        }
    }
}
