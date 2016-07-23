//! Bucket containing objects of the same age
//!
//! A Bucket contains a list of blocks with objects of the same age. This allows
//! tracking of an object's age without incrementing a counter for every object.

use immix::block::Block;
use object::Object;
use object_pointer::ObjectPointer;

/// Structure storing data of a single bucket.
pub struct Bucket {
    /// The memory blocks to store objects in.
    pub blocks: Vec<Block>,

    /// The number of GC cycles the objects in this bucket have survived.
    pub age: usize,

    /// Index to the current block to allocate objects into.
    pub block_index: usize,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Bucket {
        Bucket {
            blocks: Vec::new(),
            age: 0,
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

        // Don't increment the block index the first time we add a block.
        if self.blocks.len() > 1 {
            self.block_index += 1;
        }

        self.blocks.last_mut().unwrap()
    }

    /// Adds a new block and then bump allocates an object into it.
    pub fn add_block_and_bump_allocate(&mut self,
                                       block: Block,
                                       object: Object)
                                       -> ObjectPointer {
        self.add_block(block);
        self.bump_allocate(object)
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
