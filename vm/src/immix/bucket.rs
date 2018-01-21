//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.
use parking_lot::{Mutex, MutexGuard};

use deref_pointer::DerefPointer;
use immix::block_list::BlockList;
use immix::block::{Block, LINES_PER_BLOCK, MAX_HOLES};
use immix::histogram::Histogram;
use immix::global_allocator::RcGlobalAllocator;
use immix::finalization_list::FinalizationList;
use object::Object;
use object_pointer::ObjectPointer;

/// The age of a bucket containing mature objects.
pub const MATURE: isize = 100;

/// The age of a bucket containing mailbox objects.
pub const MAILBOX: isize = 200;

/// The age of a bucket containing permanent objects.
pub const PERMANENT: isize = 300;

/// Structure storing data of a single bucket.
pub struct Bucket {
    // The available space histogram for the blocks in this bucket.
    pub available_histogram: Histogram,

    /// The mark histogram for the blocks in this bucket.
    pub mark_histogram: Histogram,

    /// Lock used whenever moving objects around (e.g. when evacuating or
    /// promoting them).
    pub lock: Mutex<()>,

    /// The blocks managed by this bucket.
    pub blocks: BlockList,

    /// The current block to allocate into.
    ///
    /// This pointer may be NULL to indicate no block is present yet.
    pub current_block: DerefPointer<Block>,

    /// The age of the objects in the current bucket.
    pub age: isize,

    /// The objects in this bucket should be promoted to the mature generation.
    pub promote: bool,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Self {
        Self::with_age(0)
    }

    pub fn with_age(age: isize) -> Self {
        Bucket {
            blocks: BlockList::new(),
            current_block: DerefPointer::null(),
            age: age,
            available_histogram: Histogram::new(MAX_HOLES),
            mark_histogram: Histogram::new(LINES_PER_BLOCK),
            promote: false,
            lock: Mutex::new(()),
        }
    }

    pub fn lock(&self) -> MutexGuard<()> {
        self.lock.lock()
    }

    pub fn reset_age(&mut self) {
        self.age = 0;
        self.promote = false;
    }

    pub fn increment_age(&mut self) {
        self.age += 1;
    }

    pub fn number_of_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn current_block(&self) -> Option<&DerefPointer<Block>> {
        if self.current_block.is_null() {
            None
        } else {
            Some(&self.current_block)
        }
    }

    pub fn current_block_mut(&mut self) -> Option<&mut DerefPointer<Block>> {
        if self.current_block.is_null() {
            None
        } else {
            Some(&mut self.current_block)
        }
    }

    pub fn add_block(&mut self, mut block: Box<Block>) {
        block.set_bucket(self as *mut Bucket);

        self.current_block = DerefPointer::new(&*block);
        self.blocks.push_back(block);
    }

    // Finds a hole to allocate into.
    //
    // Returns true if a hole was found.
    pub fn find_available_hole(&mut self) -> bool {
        if let Some(current) = self.current_block_mut() {
            if current.is_available_for_allocation() {
                return true;
            }
        } else {
            return false;
        }

        // We have a block but we can't allocate into it. This means we need to
        // find another block to allocate into, if there are any at all.
        if let Some(block) = self.find_next_available_block() {
            self.current_block = block;

            true
        } else {
            false
        }
    }

    pub fn find_next_available_block(&mut self) -> Option<DerefPointer<Block>> {
        if let Some(current) = self.current_block_mut() {
            for block in current.iter_mut() {
                if block.is_available_for_allocation() {
                    return Some(DerefPointer::new(block));
                }
            }
        }

        None
    }

    /// Allocates an object into this bucket
    pub fn allocate(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let found_hole = self.find_available_hole();

        if !found_hole {
            self.add_block(global_allocator.request_block());
        }

        (
            !found_hole,
            self.current_block_mut().unwrap().bump_allocate(object),
        )
    }

    /// Returns true if this bucket contains blocks that need to be evacuated.
    pub fn should_evacuate(&self) -> bool {
        // The Immix paper states that one should evacuate when there are one or
        // more recyclable or fragmented blocks. In IVM all objects are the same
        // size and thus it's not possible to have any recyclable blocks left by
        // the time we start a collection (as they have all been consumed). As
        // such we don't check for these and instead only check for fragmented
        // blocks.
        self.blocks.iter().any(|block| block.is_fragmented())
    }

    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    pub fn reclaim_blocks(&mut self) -> (BlockList, FinalizationList) {
        let mut reclaim = BlockList::new();
        let mut finalize = FinalizationList::new();

        self.available_histogram.reset();
        self.mark_histogram.reset();

        for mut block in self.blocks.drain() {
            block.update_line_map();
            block.push_pointers_to_finalize(&mut finalize);

            if block.is_empty() {
                block.reset();
                reclaim.push_back(block);
            } else {
                let holes = block.update_hole_count();

                if holes > 0 {
                    self.mark_histogram
                        .increment(holes, block.marked_lines_count());

                    block.recycle();
                }

                self.blocks.push_back(block);
            }
        }

        self.current_block = self.blocks
            .head()
            .map(|block| DerefPointer::new(block))
            .unwrap_or_else(|| DerefPointer::null());

        (reclaim, finalize)
    }

    /// Prepares this bucket for a collection.
    ///
    /// Returns true if evacuation is needed for this bucket.
    pub fn prepare_for_collection(&mut self) -> bool {
        let mut available: isize = 0;
        let mut required: isize = 0;
        let evacuate = self.should_evacuate();

        for block in self.blocks.iter_mut() {
            let holes = block.holes();

            if evacuate && holes > 0 {
                let count = block.available_lines_count();

                self.available_histogram.increment(holes, count);

                available += count as isize;
            }

            block.prepare_for_collection();
        }

        if available > 0 {
            let mut iter = self.mark_histogram.iter();
            let mut min_bin = None;

            while available > required {
                if let Some(bin) = iter.next() {
                    required += self.mark_histogram.get(bin) as isize;
                    available -= self.available_histogram.get(bin) as isize;

                    min_bin = Some(bin);
                } else {
                    break;
                }
            }

            if let Some(bin) = min_bin {
                for block in self.blocks.iter_mut() {
                    if block.holes() >= bin {
                        block.set_fragmented();
                    }
                }
            }
        }

        evacuate
    }
}

#[cfg(test)]
use std::ops::Drop;

#[cfg(test)]
impl Drop for Bucket {
    fn drop(&mut self) {
        // To prevent memory leaks in the tests we automatically finalize any
        // data, removing the need for doing this manually in every test.
        for mut block in self.blocks.drain() {
            block.reset_mark_bitmaps();
            block.finalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::block::Block;
    use immix::bitmap::Bitmap;
    use immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
    use object::Object;
    use object_value;

    fn global_allocator() -> RcGlobalAllocator {
        GlobalAllocator::new()
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

        bucket.promote = true;
        bucket.reset_age();

        assert_eq!(bucket.age, 0);
        assert_eq!(bucket.promote, false);
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
        assert_eq!(bucket.current_block.is_null(), false);
        assert!(bucket.blocks[0].bucket().is_some());

        assert!(
            &*bucket.current_block as *const Block
                == &bucket.blocks[0] as *const Block
        );

        bucket.add_block(Block::new());

        assert_eq!(bucket.blocks.len(), 2);

        assert!(
            &*bucket.current_block as *const Block
                == &bucket.blocks[1] as *const Block
        );
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

        assert!(
            block.free_pointer == unsafe { block.start_address().offset(4) }
        );
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

        assert!(
            block.free_pointer == unsafe { block.start_address().offset(1) }
        );

        bucket.allocate(&global_alloc, Object::new(object_value::none()));

        assert!(
            block.free_pointer == unsafe { block.start_address().offset(2) }
        );
    }

    #[test]
    fn test_allocate_with_recyclable_blocks() {
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();

        let (_, pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::none()));

        pointer.mark();

        bucket.reclaim_blocks();

        assert_eq!(bucket.blocks.len(), 1);

        let (new_block, new_pointer) = bucket
            .allocate(&global_alloc, Object::new(object_value::float(4.0)));

        assert_eq!(new_block, false);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_float());

        let head = bucket.blocks.head().unwrap();

        assert!(head.free_pointer == unsafe { head.start_address().offset(5) });
    }

    #[test]
    fn test_should_evacuate_with_fragmented_blocks() {
        let mut bucket = Bucket::new();
        let mut block = Block::new();

        block.set_fragmented();

        bucket.add_block(block);

        assert!(bucket.should_evacuate());
    }

    #[test]
    fn test_reclaim_blocks() {
        let mut bucket = Bucket::new();
        let mut block1 = Block::new();
        let block2 = Block::new();
        let mut block3 = Block::new();

        block1.used_lines_bitmap.set(255);
        block3.used_lines_bitmap.set(2);

        bucket.add_block(block1);
        bucket.add_block(block2);
        bucket.add_block(block3);
        bucket.reclaim_blocks();

        assert_eq!(bucket.blocks.len(), 2);

        assert_eq!(bucket.blocks[0].holes(), 1);
        assert_eq!(bucket.blocks[1].holes(), 2);

        assert_eq!(bucket.mark_histogram.get(1), 1);
        assert_eq!(bucket.mark_histogram.get(2), 1);
    }

    #[test]
    fn test_reclaim_blocks_full() {
        let mut bucket = Bucket::new();
        let mut block = Block::new();

        for i in 0..256 {
            block.used_lines_bitmap.set(i);
        }

        bucket.add_block(block);
        bucket.reclaim_blocks();

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.current_block.is_null(), false);
    }

    #[test]
    fn test_prepare_for_collection_without_evacuation() {
        let mut bucket = Bucket::new();

        bucket.add_block(Block::new());
        bucket.current_block_mut().unwrap().used_lines_bitmap.set(1);

        assert_eq!(bucket.prepare_for_collection(), false);

        // No evacuation needed means the available histogram is not updated.
        assert_eq!(bucket.available_histogram.get(1), 0);

        let block = bucket.current_block().unwrap();

        assert!(block.marked_objects_bitmap.is_empty());
    }

    #[test]
    fn test_prepare_for_collection_with_evacuation() {
        let mut bucket = Bucket::new();
        let mut block1 = Block::new();
        let block2 = Block::new();

        block1.used_lines_bitmap.set(1);
        block1.set_fragmented();

        bucket.add_block(block1);
        bucket.add_block(block2);

        // Normally the collector updates the mark histogram at the end of a
        // cycle. Since said code is not executed by the function we're testing
        // we'll update this histogram manually.
        bucket.mark_histogram.increment(1, 1);

        assert!(bucket.prepare_for_collection());

        assert_eq!(bucket.available_histogram.get(1), 509);

        let block = bucket.current_block().unwrap();

        assert!(block.is_fragmented());
        assert!(block.marked_objects_bitmap.is_empty());
    }
}
