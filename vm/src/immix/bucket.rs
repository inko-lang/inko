//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.
use parking_lot::{Mutex, MutexGuard};

use std::ptr;
use std::iter::Chain;
use std::slice::IterMut;

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

    /// The objects in this bucket should be promoted to the mature generation.
    pub promote: bool,

    /// Lock used whenever moving objects around (e.g. when evacuating or
    /// promoting them).
    pub lock: Mutex<()>,
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
        let block = self.current_block_mut().unwrap();

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
    pub fn allocate(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let found_hole = self.find_available_hole();

        if !found_hole {
            if let Some(block) = self.recyclable_blocks.pop() {
                self.add_block(block);
            } else {
                self.add_block(global_allocator.request_block());
            }
        }

        (
            !found_hole,
            self.current_block_mut().unwrap().bump_allocate(object),
        )
    }

    /// Returns true if this bucket contains blocks that need to be evacuated.
    pub fn should_evacuate(&self) -> bool {
        if self.recyclable_blocks.len() > 0 {
            return true;
        }

        // TODO: use a counter instead of iterating over blocks
        for block in self.blocks.iter() {
            if block.fragmented {
                return true;
            }
        }

        false
    }

    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    pub fn reclaim_blocks(&mut self) -> (Vec<Box<Block>>, FinalizationList) {
        let mut reclaim = Vec::new();
        let mut finalize = FinalizationList::new();

        self.available_histogram.reset();
        self.mark_histogram.reset();

        let blocks = self.blocks
            .drain(0..)
            .chain(self.recyclable_blocks.drain(0..))
            .collect::<Vec<Box<Block>>>();

        for mut block in blocks {
            block.update_line_map();
            block.push_pointers_to_finalize(&mut finalize);

            if block.is_empty() {
                block.reset();
                reclaim.push(block);
            } else {
                block.update_hole_count();

                if block.holes > 0 {
                    self.mark_histogram
                        .increment(block.holes, block.marked_lines_count());

                    // Recyclable blocks should be stored separately.
                    if !block.fragmented {
                        block.recycle();
                        self.recyclable_blocks.push(block);

                        continue;
                    }
                }

                self.blocks.push(block);
            }
        }

        // At this point "self.blocks" only contains either full or fragmented
        // blocks.
        self.current_block = ptr::null::<Block>() as *mut Block;

        (reclaim, finalize)
    }

    /// Prepares this bucket for a collection.
    ///
    /// Returns true if evacuation is needed for this bucket.
    pub fn prepare_for_collection(&mut self) -> bool {
        let mut available: isize = 0;
        let mut required: isize = 0;
        let evacuate = self.should_evacuate();

        for block in self.blocks
            .iter_mut()
            .chain(self.recyclable_blocks.iter_mut())
        {
            if evacuate && block.holes > 0 {
                let count = block.available_lines_count();

                self.available_histogram.increment(block.holes, count);

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

        evacuate
    }

    /// Returns a mutable iterator for all blocks.
    pub fn all_blocks_mut(
        &mut self,
    ) -> Chain<IterMut<Box<Block>>, IterMut<Box<Block>>> {
        self.blocks
            .iter_mut()
            .chain(self.recyclable_blocks.iter_mut())
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

        assert_eq!(bucket.recyclable_blocks.len(), 1);
        assert_eq!(bucket.blocks.len(), 0);

        let (new_block, new_pointer) = bucket
            .allocate(&global_alloc, Object::new(object_value::float(4.0)));

        assert!(new_block);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_float());

        assert!(
            bucket.blocks[0].free_pointer == unsafe {
                bucket.blocks[0].start_address().offset(5)
            }
        );
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

        assert_eq!(bucket.mark_histogram.get(1), 1);
        assert_eq!(bucket.mark_histogram.get(2), 1);
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

        assert_eq!(bucket.prepare_for_collection(), false);

        // No evacuation needed means the available histogram is not updated.
        assert_eq!(bucket.available_histogram.get(1), 0);

        let block = bucket.current_block().unwrap();

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

        assert!(bucket.prepare_for_collection());

        assert_eq!(bucket.available_histogram.get(1), 509);

        let block = bucket.current_block().unwrap();

        assert!(block.fragmented);
        assert!(block.marked_objects_bitmap.is_empty());
    }
}
