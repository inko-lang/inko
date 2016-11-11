//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.

use std::ptr;

use immix::block::Block;
use immix::histogram::Histogram;
use immix::global_allocator::RcGlobalAllocator;
use object::Object;
use object_pointer::ObjectPointer;

/// Structure storing data of a single bucket.
pub struct Bucket {
    /// Blocks to allocate into.
    ///
    /// At the end of a collection cycle all these blocks are either full or
    /// marked for evacuation.
    pub blocks: Vec<Box<Block>>,

    /// Blocks that can be recycled by the allocator.
    ///
    /// These blocks _may_ still contain live objects.
    pub recyclable_blocks: Vec<Box<Block>>,

    /// The current block to allocate into.
    ///
    /// This pointer may be NULL to indicate no block is present yet.
    pub current_block: *mut Block,

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
            recyclable_blocks: Vec::new(),
            current_block: ptr::null::<Block>() as *mut Block,
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

    pub fn current_block(&self) -> Option<&Block> {
        if self.current_block.is_null() {
            None
        } else {
            Some(unsafe { &*self.current_block })
        }
    }

    pub fn current_block_mut(&mut self) -> Option<&mut Block> {
        if self.current_block.is_null() {
            None
        } else {
            Some(unsafe { &mut *self.current_block })
        }
    }

    pub fn add_block(&mut self, block: Box<Block>) {
        self.current_block = &*block as *const Block as *mut Block;

        self.blocks.push(block);

        let block_ptr = self as *mut Bucket;
        let mut block = self.current_block_mut().unwrap();

        block.set_bucket(block_ptr);
    }

    // Finds a hole to allocate into.
    //
    // Returns true if a hole was found.
    pub fn find_available_hole(&mut self) -> bool {
        if let Some(block) = self.current_block_mut() {
            if block.can_bump_allocate() {
                true
            } else {
                // Hole consumed, try to find a new one in the current block
                block.find_available_hole();

                block.can_bump_allocate()
            }
        } else {
            false
        }
    }

    /// Allocates an object into this bucket
    pub fn allocate(&mut self,
                    global_allocator: &RcGlobalAllocator,
                    object: Object)
                    -> (bool, ObjectPointer) {
        let found_hole = self.find_available_hole();

        if !found_hole {
            if let Some(block) = self.recyclable_blocks.pop() {
                if block.is_fragmented() {
                    panic!("recyclable block is fragmented");
                }

                self.add_block(block);
            } else {
                self.add_block(global_allocator.request_block());
            }
        }

        (!found_hole, self.current_block_mut().unwrap().bump_allocate(object))
    }

    /// Returns true if this bucket contains blocks that need to be evacuated.
    pub fn should_evacuate(&self) -> bool {
        if self.recyclable_blocks.len() > 0 {
            return true;
        }

        // TODO: use a counter instead of iterating over blocks
        for block in self.blocks.iter() {
            if block.is_fragmented() {
                return true;
            }
        }

        false
    }

    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    pub fn reclaim_blocks(&mut self) -> Vec<Box<Block>> {
        let mut keep = Vec::new();
        let mut recycle = Vec::new();
        let mut reclaim = Vec::new();

        self.available_histogram.reset();
        self.mark_histogram.reset();

        for mut block in self.blocks
            .drain(0..)
            .chain(self.recyclable_blocks.drain(0..)) {
            if block.is_empty() {
                block.reset();
                reclaim.push(block);
            } else {
                block.update_hole_count();

                if block.holes > 0 {
                    self.mark_histogram
                        .increment(block.holes, block.marked_lines_count());

                    // Recyclable blocks should be stored separately.
                    if !block.is_fragmented() {
                        block.recycle();
                        recycle.push(block);

                        continue;
                    }
                }

                keep.push(block);
            }
        }

        self.blocks = keep;
        self.recyclable_blocks = recycle;

        // At this point "self.blocks" only contains either full or fragmented
        // blocks.
        self.current_block = ptr::null::<Block>() as *mut Block;

        reclaim
    }

    /// Prepares this bucket for a collection.
    pub fn prepare_for_collection(&mut self) {
        let mut available: isize = 0;
        let mut required: isize = 0;
        let evacuate = self.should_evacuate();

        for block in self.blocks
            .iter_mut()
            .chain(self.recyclable_blocks.iter_mut()) {
            if evacuate && block.holes > 0 {
                let count = block.available_lines_count();

                self.available_histogram.increment(block.holes, count);

                available += count as isize;
            }

            block.reset_bitmaps();
        }

        if available > 0 {
            let mut iter = self.mark_histogram.iter();
            let mut min_bin = None;

            while available > required {
                if let Some(bin) = iter.next() {
                    required += self.mark_histogram.get(bin).unwrap() as isize;

                    available -=
                        self.available_histogram.get(bin).unwrap() as isize;

                    min_bin = Some(bin);
                } else {
                    break;
                }
            }

            if let Some(bin) = min_bin {
                for mut block in self.blocks.iter_mut() {
                    if block.holes >= bin {
                        block.set_fragmented();
                    }
                }

                // Recyclable blocks that need to be evacuated have to be moved
                // to the regular list of blocks.
                let mut recyclable = Vec::new();

                for mut block in self.recyclable_blocks.drain(0..) {
                    if block.holes >= bin {
                        block.set_fragmented();
                        self.blocks.push(block);
                    } else {
                        recyclable.push(block);
                    }
                }

                self.recyclable_blocks = recyclable;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::block::Block;
    use immix::bitmap::Bitmap;
    use immix::global_allocator::{RcGlobalAllocator, GlobalAllocator};
    use object::Object;
    use object_value;

    fn global_allocator() -> RcGlobalAllocator {
        GlobalAllocator::without_preallocated_blocks()
    }

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
        assert!(bucket.current_block.is_null());
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
    fn test_current_block_without_block() {
        let bucket = Bucket::new();

        assert!(bucket.current_block().is_none());
    }

    #[test]
    fn test_current_block_with_block() {
        let mut bucket = Bucket::new();
        let block = Block::new();

        bucket.add_block(block);

        assert!(bucket.current_block().is_some());
    }

    #[test]
    fn test_current_block_mut_without_block() {
        let mut bucket = Bucket::new();

        assert!(bucket.current_block_mut().is_none());
    }

    #[test]
    fn test_current_block_mut() {
        let mut bucket = Bucket::new();
        let block = Block::new();

        bucket.add_block(block);

        assert!(bucket.current_block_mut().is_some());
    }

    #[test]
    fn test_add_block() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.blocks[0].bucket.is_null(), false);

        assert!(bucket.current_block == &mut *bucket.blocks[0] as *mut Block);

        bucket.add_block(Block::new());

        assert_eq!(bucket.blocks.len(), 2);

        assert!(bucket.current_block == &mut *bucket.blocks[1] as *mut Block);
    }

    #[test]
    fn test_find_available_hole_first_line_free() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        assert!(bucket.find_available_hole());

        let block = bucket.current_block().unwrap();

        assert!(block.free_pointer == block.start_address());
    }

    #[test]
    fn test_find_available_hole_first_line_full() {
        let mut bucket = Bucket::new();

        {
            let mut block = Block::new();

            block.used_lines_bitmap.set(1);
            block.find_available_hole();
            bucket.add_block(block);
        }

        assert!(bucket.find_available_hole());

        let block = bucket.current_block().unwrap();

        assert!(block.free_pointer == unsafe { block.start_address().offset(4) });
    }

    #[test]
    fn test_allocate_without_blocks() {
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();

        let (new_block, pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::none()));

        assert!(new_block);
        assert!(pointer.get().value.is_none());

        let block = pointer.block();

        assert!(block.free_pointer == unsafe { block.start_address().offset(1) });

        bucket.allocate(&global_alloc, Object::new(object_value::none()));

        assert!(block.free_pointer == unsafe { block.start_address().offset(2) });
    }

    #[test]
    fn test_allocate_with_recyclable_blocks() {
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();

        let (_, pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::none()));

        pointer.mark();

        bucket.reclaim_blocks();

        assert_eq!(bucket.recyclable_blocks.len(), 1);
        assert_eq!(bucket.blocks.len(), 0);

        let (new_block, new_pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::integer(4)));

        assert!(new_block);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_integer());

        assert!(bucket.blocks[0].free_pointer ==
                unsafe { bucket.blocks[0].start_address().offset(5) });
    }

    #[test]
    fn test_should_evacuate_with_recyclable_blocks() {
        let mut bucket = Bucket::new();

        bucket.recyclable_blocks.push(Block::new());

        assert!(bucket.should_evacuate());
    }

    #[test]
    fn test_should_evacuate_with_fragmented_blocks() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        bucket.blocks[0].set_fragmented();

        assert!(bucket.should_evacuate());
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

        assert_eq!(bucket.blocks.len(), 0);
        assert_eq!(bucket.recyclable_blocks.len(), 2);

        assert_eq!(bucket.recyclable_blocks[0].holes, 1);
        assert_eq!(bucket.recyclable_blocks[1].holes, 2);

        assert_eq!(bucket.mark_histogram.get(1).unwrap(), 1);
        assert_eq!(bucket.mark_histogram.get(2).unwrap(), 1);
    }

    #[test]
    fn test_reclaim_blocks_full() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());

        for i in 0..256 {
            bucket.blocks[0].used_lines_bitmap.set(i);
        }

        bucket.reclaim_blocks();

        assert_eq!(bucket.blocks.len(), 1);
        assert!(bucket.current_block.is_null());
    }

    #[test]
    fn test_prepare_for_collection_without_evacuation() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.current_block_mut().unwrap().used_lines_bitmap.set(1);
        bucket.prepare_for_collection();

        // No evacuation needed means the available histogram is not updated.
        assert!(bucket.available_histogram.get(1).is_none());

        let block = bucket.current_block().unwrap();

        assert!(block.used_lines_bitmap.is_empty());
        assert!(block.marked_objects_bitmap.is_empty());
    }

    #[test]
    fn test_prepare_for_collection_with_evacuation() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.recyclable_blocks.push(Block::new());
        bucket.current_block_mut().unwrap().used_lines_bitmap.set(1);

        // Normally the collector updates the mark histogram at the end of a
        // cycle. Since said code is not executed by the function we're testing
        // we'll update this histogram manually.
        bucket.mark_histogram.increment(1, 1);
        bucket.prepare_for_collection();

        assert_eq!(bucket.available_histogram.get(1).unwrap(), 509);

        let block = bucket.current_block().unwrap();

        assert!(block.is_fragmented());
        assert!(block.used_lines_bitmap.is_empty());
        assert!(block.marked_objects_bitmap.is_empty());
    }
}
