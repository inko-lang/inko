//! Global Allocator for requesting and re-using free blocks.
//!
//! The global allocator is used by process-local allocators to request the
//! allocation of new blocks or the re-using of existing (and returned) free
//! blocks.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::immix::block::Block;
use crate::immix::block_list::BlockList;
use parking_lot::Mutex;

pub type RcGlobalAllocator = ArcWithoutWeak<GlobalAllocator>;

/// Structure used for storing the state of the global allocator.
pub struct GlobalAllocator {
    blocks: Mutex<BlockList>,
}

impl GlobalAllocator {
    /// Creates a new GlobalAllocator with a number of blocks pre-allocated.
    pub fn with_rc() -> RcGlobalAllocator {
        ArcWithoutWeak::new(GlobalAllocator {
            blocks: Mutex::new(BlockList::new()),
        })
    }

    /// Requests a new free block from the pool
    pub fn request_block(&self) -> Box<Block> {
        if let Some(block) = self.blocks.lock().pop() {
            block
        } else {
            Block::boxed()
        }
    }

    /// Adds a block to the pool so it can be re-used.
    pub fn add_block(&self, block: Box<Block>) {
        self.blocks.lock().push(block);
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
    use crate::immix::block_list::BlockList;

    #[test]
    fn test_new() {
        let alloc = GlobalAllocator::with_rc();

        assert!(alloc.blocks.lock().pop().is_none());
    }

    #[test]
    fn test_request_block() {
        let alloc = GlobalAllocator::with_rc();
        let block = alloc.request_block();

        alloc.add_block(block);
        alloc.request_block();

        assert!(alloc.blocks.lock().pop().is_none());
    }

    #[test]
    fn test_add_block() {
        let alloc = GlobalAllocator::with_rc();
        let block = alloc.request_block();

        alloc.add_block(block);

        assert!(alloc.blocks.lock().pop().is_some());
    }

    #[test]
    fn test_add_blocks() {
        let alloc = GlobalAllocator::with_rc();
        let mut blocks = BlockList::new();

        blocks.push(alloc.request_block());
        blocks.push(alloc.request_block());
        alloc.add_blocks(&mut blocks);

        assert_eq!(alloc.blocks.lock().len(), 2);
    }
}
