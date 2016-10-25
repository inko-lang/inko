//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.

use immix::block::Block;
use immix::histogram::Histogram;
use object::Object;
use object_pointer::ObjectPointer;

/// Structure storing data of a single bucket.
pub struct Bucket {
    /// The memory blocks to store objects in.
    pub blocks: Vec<Box<Block>>,

    /// Index to the current block to allocate objects into.
    pub block_index: usize,

    /// The age of the objects in the current bucket.
    pub age: isize,

    // The available space histogram for the blocks in this bucket.
    pub available_histogram: Histogram,

    /// The mark histogram for the blocks in this bucket.
    pub mark_histogram: Histogram,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Self {
        Self::with_age(0)
    }

    /// Returns a Bucket with a custom age.
    pub fn with_age(age: isize) -> Self {
        Bucket {
            blocks: Vec::new(),
            block_index: 0,
            age: age,
            available_histogram: Histogram::new(),
            mark_histogram: Histogram::new(),
        }
    }

    pub fn reset_age(&mut self) {
        self.age = 0;
    }

    pub fn increment_age(&mut self) {
        self.age += 1;
    }

    /// Returns an immutable reference to the current block.
    pub fn current_block(&self) -> &Box<Block> {
        self.blocks.get(self.block_index).unwrap()
    }

    /// Returns a mutable reference to the current block.
    pub fn current_block_mut(&mut self) -> &mut Box<Block> {
        self.blocks.get_mut(self.block_index).unwrap()
    }

    /// Adds a new block to the current bucket.
    pub fn add_block(&mut self, block: Box<Block>) -> &mut Box<Block> {
        self.blocks.push(block);

        self.block_index = self.blocks.len() - 1;

        let block_ptr = self as *mut Bucket;
        let mut block_ref = self.blocks.last_mut().unwrap();

        block_ref.set_bucket(block_ptr);

        block_ref
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
    pub fn first_available_block(&mut self) -> Option<&mut Box<Block>> {
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
            } else {
                block.set_full();
            }

            // The entire block has been consumed so we'll try the next one.
        }

        None
    }

    /// Resets the block to use for allocations to the first available block.
    pub fn rewind_allocator(&mut self) {
        self.block_index = 0;

        self.first_available_block();
    }

    /// Returns true if this bucket contains any recyclable blocks.
    pub fn has_recyclable_blocks(&self) -> bool {
        for block in self.blocks.iter() {
            if block.is_recyclable() {
                return true;
            }
        }

        false
    }

    /// Removes and returns all unused blocks from this bucket.
    ///
    /// For blocks that are kept around the hole count and the mark histogram is
    /// updated.
    pub fn reclaim_blocks(&mut self) -> Vec<Box<Block>> {
        let mut keep = Vec::new();
        let mut reclaim = Vec::new();

        self.available_histogram.reset();
        self.mark_histogram.reset();

        for mut block in self.blocks.drain(0..) {
            if block.is_empty() {
                block.reset();
                reclaim.push(block);
            } else {
                block.update_hole_count();

                self.mark_histogram
                    .increment(block.holes, block.marked_lines_count());

                if block.holes > 0 {
                    block.set_recyclable();
                }

                keep.push(block);
            }
        }

        self.blocks = keep;

        reclaim
    }
}
