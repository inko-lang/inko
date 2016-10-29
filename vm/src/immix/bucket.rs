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

    pub fn current_block(&self) -> &Box<Block> {
        self.blocks.get(self.block_index).unwrap()
    }

    pub fn current_block_mut(&mut self) -> &mut Box<Block> {
        self.blocks.get_mut(self.block_index).unwrap()
    }

    pub fn add_block(&mut self, block: Box<Block>) -> &mut Box<Block> {
        self.blocks.push(block);

        self.block_index = self.blocks.len() - 1;

        let block_ptr = self as *mut Bucket;
        let mut block_ref = self.blocks.last_mut().unwrap();

        block_ref.set_bucket(block_ptr);

        block_ref
    }

    pub fn bump_allocate(&mut self, object: Object) -> ObjectPointer {
        self.current_block_mut().bump_allocate(object)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use immix::block::Block;
    use immix::bitmap::Bitmap;
    use object::Object;
    use object_value::ObjectValue;

    #[test]
    fn test_new() {
        let bucket = Bucket::new();

        assert_eq!(bucket.age, 0);
    }

    #[test]
    fn test_with_age() {
        let bucket = Bucket::with_age(4);

        assert_eq!(bucket.age, 4);
        assert_eq!(bucket.blocks.len(), 0);
        assert_eq!(bucket.block_index, 0);
    }

    #[test]
    fn test_reset_age() {
        let mut bucket = Bucket::with_age(4);

        bucket.reset_age();

        assert_eq!(bucket.age, 0);
    }

    #[test]
    fn test_increment_age() {
        let mut bucket = Bucket::new();

        bucket.increment_age();
        bucket.increment_age();

        assert_eq!(bucket.age, 2);
    }

    #[test]
    #[should_panic]
    fn test_current_block_without_block() {
        let bucket = Bucket::new();

        bucket.current_block();
    }

    #[test]
    fn test_current_block_with_block() {
        let mut bucket = Bucket::new();
        let block = Block::new();

        bucket.add_block(block);
        bucket.current_block();
    }

    #[test]
    #[should_panic]
    fn test_current_block_mut_without_block() {
        let mut bucket = Bucket::new();

        bucket.current_block_mut();
    }

    #[test]
    fn test_current_block_mut() {
        let mut bucket = Bucket::new();
        let block = Block::new();

        bucket.add_block(block);
        bucket.current_block_mut();
    }

    #[test]
    fn test_add_block() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.blocks[0].bucket.is_null(), false);
    }

    #[test]
    fn test_bump_allocate() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        let ptr = bucket.bump_allocate(Object::new(ObjectValue::Integer(1)));

        assert!(ptr.get().value.is_integer());
    }

    #[test]
    fn test_can_bump_allocate() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        assert!(bucket.can_bump_allocate());
    }

    #[test]
    fn test_first_available_block_first_block_empty() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        bucket.first_available_block();

        assert_eq!(bucket.block_index, 0);
    }

    #[test]
    fn test_first_available_block_first_block_unavailable() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.add_block(Block::new());

        bucket.block_index = 0;

        bucket.blocks[0].set_full();
        bucket.first_available_block();

        assert_eq!(bucket.block_index, 1);
    }

    #[test]
    fn test_first_available_block_first_block_consumed() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.add_block(Block::new());

        bucket.block_index = 0;

        bucket.blocks[0].free_pointer = bucket.blocks[0].end_pointer;
        bucket.first_available_block();

        assert_eq!(bucket.block_index, 1);
    }

    #[test]
    fn test_rewind_allocator() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.add_block(Block::new());
        bucket.rewind_allocator();

        assert_eq!(bucket.block_index, 0);

        assert!(bucket.blocks[0].free_pointer ==
                bucket.blocks[0].start_address());
    }

    #[test]
    fn test_has_recyclable_blocks() {
        let mut bucket = Bucket::new();

        assert_eq!(bucket.has_recyclable_blocks(), false);

        bucket.add_block(Block::new());

        assert_eq!(bucket.has_recyclable_blocks(), false);

        bucket.blocks[0].set_recyclable();

        assert!(bucket.has_recyclable_blocks());
    }

    #[test]
    fn test_reclaim_blocks() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.add_block(Block::new());
        bucket.add_block(Block::new());

        bucket.blocks[0].used_lines_bitmap.set(255);
        bucket.blocks[2].used_lines_bitmap.set(2);

        bucket.reclaim_blocks();

        assert_eq!(bucket.blocks.len(), 2);

        assert_eq!(bucket.blocks[0].holes, 1);
        assert_eq!(bucket.blocks[1].holes, 2);

        assert!(bucket.blocks[0].is_recyclable());
        assert!(bucket.blocks[1].is_recyclable());

        assert_eq!(bucket.mark_histogram.get(1).unwrap(), 1);
        assert_eq!(bucket.mark_histogram.get(2).unwrap(), 1);
    }
}
