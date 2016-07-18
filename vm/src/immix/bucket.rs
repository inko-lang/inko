//! Bucket containing objects of the same age
//!
//! A Bucket contains a list of blocks with objects of the same age. This allows
//! tracking of an object's age without incrementing a counter for every object.

use immix::block::Block;

/// Structure storing data of a single bucket.
pub struct Bucket {
    pub blocks: Vec<Block>,
    pub age: usize,
}

impl Bucket {
    pub fn new() -> Bucket {
        Bucket {
            blocks: Vec::new(),
            age: 0,
        }
    }

    /// Adds a new Block and returns a mutable reference to it.
    pub fn add_block(&mut self, block: Block) -> &mut Block {
        self.blocks.push(block);

        self.blocks.last_mut().unwrap()
    }

    /// Attempts to find a block to allocate into.
    pub fn find_block(&mut self) -> Option<&mut Block> {
        let mut found = None;

        for block in self.blocks.iter_mut() {
            if block.is_available() {
                found = Some(block);
                break;
            }
        }

        found
    }
}
