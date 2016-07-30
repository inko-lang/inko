//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.

use immix::block::Block;
use object::Object;
use object_pointer::ObjectPointer;

/// Structure storing data of a single bucket.
pub struct Bucket {
    /// The memory blocks to store objects in.
    pub blocks: Vec<Block>,

    /// Index to the current block to allocate objects into.
    pub block_index: usize,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Bucket {
        Bucket {
            blocks: Vec::new(),
            block_index: 0,
        }
    }

    /// Returns an immutable reference to the current block.
    pub fn current_block(&self) -> &Block {
        self.blocks.get(self.block_index).unwrap()
    }

    /// Returns a mutable reference to the current block.
    pub fn current_block_mut(&mut self) -> &mut Block {
        self.blocks.get_mut(self.block_index).unwrap()
    }

    /// Adds a new block to the current bucket.
    pub fn add_block(&mut self, block: Block) -> &mut Block {
        self.blocks.push(block);

        self.block_index = self.blocks.len() - 1;

        self.blocks.last_mut().unwrap()
    }

    /// Bump allocates into the current block.
    pub fn bump_allocate(&mut self, object: Object) -> ObjectPointer {
        self.current_block_mut().bump_allocate(object)
    }

    /// Returns true if we can bump allocate into the current block.
    pub fn can_bump_allocate(&self) -> bool {
        self.current_block().can_bump_allocate()
    }

    /// Attempts to find the first available block that we can allocate into.
    ///
    /// Once a block has been found we store the index so any further
    /// allocations use this block when possible.
    pub fn first_available_block(&mut self) -> Option<&mut Block> {
        let start = self.block_index;

        // Attempt to find any available blocks after the current one.
        for (index, block) in self.blocks[start..].iter_mut().enumerate() {
            if !block.is_available() {
                continue;
            }

            // We can bump allocate directly into the current hole.
            if block.can_bump_allocate() {
                self.block_index = index;

                return Some(block);
            }

            // The block _is_ available but the current hole has been exhausted.
            block.find_available_hole();

            if block.can_bump_allocate() {
                self.block_index = index;

                return Some(block);
            }

            // The entire block has been consumed so we'll try the next one.
        }

        None
    }
}
