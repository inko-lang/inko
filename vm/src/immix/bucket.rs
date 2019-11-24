//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.
use crate::deref_pointer::DerefPointer;
use crate::immix::block::{Block, MAX_HOLES};
use crate::immix::block_list::BlockList;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::immix::histogram::MINIMUM_BIN;
use crate::immix::histograms::Histograms;
use crate::object::Object;
use crate::object_pointer::ObjectPointer;
use crate::vm::state::State;
use parking_lot::Mutex;
use std::cell::UnsafeCell;

macro_rules! lock_bucket {
    ($bucket: expr) => {
        unsafe { &*$bucket.lock.get() }.lock()
    };
}

/// The age of a bucket containing mature objects.
pub const MATURE: i8 = 125;

/// The age of a bucket containing mailbox objects.
pub const MAILBOX: i8 = 126;

/// The age of a bucket containing permanent objects.
pub const PERMANENT: i8 = 127;

/// Structure storing data of a single bucket.
pub struct Bucket {
    /// Lock used whenever moving objects around (e.g. when evacuating or
    /// promoting them).
    pub lock: UnsafeCell<Mutex<()>>,

    /// The blocks managed by this bucket.
    pub blocks: BlockList,

    /// The current block to allocate into.
    ///
    /// This pointer may be NULL to indicate no block is present yet.
    pub current_block: DerefPointer<Block>,

    /// The age of the objects in the current bucket.
    pub age: i8,
}

unsafe impl Send for Bucket {}
unsafe impl Sync for Bucket {}

impl Bucket {
    pub fn new() -> Self {
        Self::with_age(0)
    }

    pub fn with_age(age: i8) -> Self {
        Bucket {
            blocks: BlockList::new(),
            current_block: DerefPointer::null(),
            age,
            lock: UnsafeCell::new(Mutex::new(())),
        }
    }

    pub fn reset_age(&mut self) {
        self.age = 0;
    }

    pub fn increment_age(&mut self) {
        self.age += 1;
    }

    pub fn current_block(&self) -> Option<DerefPointer<Block>> {
        let pointer = self.current_block.atomic_load();

        if pointer.is_null() {
            None
        } else {
            Some(pointer)
        }
    }

    pub fn has_current_block(&self) -> bool {
        self.current_block().is_some()
    }

    pub fn set_current_block(&mut self, block: DerefPointer<Block>) {
        self.current_block.atomic_store(block.pointer);
    }

    pub fn add_block(&mut self, mut block: Box<Block>) {
        block.set_bucket(self as *mut Bucket);

        self.set_current_block(DerefPointer::new(&*block));
        self.blocks.push(block);
    }

    pub fn reset_current_block(&mut self) {
        self.set_current_block(self.blocks.head());
    }

    /// Allocates an object into this bucket
    ///
    /// The return value is a tuple containing a boolean that indicates if a new
    /// block was requested, and the pointer to the allocated object.
    ///
    /// This method can safely be used concurrently by different threads.
    pub fn allocate(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let mut new_block = false;

        loop {
            let mut advance_block = false;
            let started_at = self.current_block.atomic_load();

            if let Some(current) = self.current_block() {
                for mut block in current.iter() {
                    if block.is_fragmented() {
                        // The block is fragmented, so skip it. The next time we
                        // find an available block we'll set it as the current
                        // block.
                        advance_block = true;

                        continue;
                    }

                    if let Some(raw_pointer) = block.request_pointer() {
                        if advance_block {
                            let _lock = lock_bucket!(self);

                            // Only advance the block if another thread didn't
                            // request a new one in the mean time.
                            self.current_block.compare_and_swap(
                                started_at.pointer,
                                &mut *block,
                            );
                        }

                        return (new_block, object.write_to(raw_pointer));
                    }
                }
            }

            // All blocks have been exhausted, or there weren't any to begin
            // with. Let's request a new one, if still necessary after obtaining
            // the lock.
            let _lock = lock_bucket!(self);

            if started_at == self.current_block.atomic_load() {
                new_block = true;
                self.add_block(global_allocator.request_block());
            }
        }
    }

    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    ///
    /// The return value is the total number of blocks after reclaiming
    /// finishes.
    pub fn reclaim_blocks(
        &mut self,
        state: &State,
        histograms: &mut Histograms,
    ) -> usize {
        let mut to_release = BlockList::new();
        let mut amount = 0;

        // We perform this work sequentially, as performing this in parallel
        // would require multiple passes over the list of input blocks. We found
        // that performing this work in parallel using Rayon ended up being
        // about 20% slower, likely due to:
        //
        // 1. The overhead of distributing work across threads.
        // 2. The list of blocks being a linked list, which can't be easily
        //    split to balance load across threads.
        for mut block in self.blocks.drain() {
            block.update_line_map();

            if block.is_empty() {
                block.reset();
                to_release.push(block);
            } else {
                let holes = block.update_hole_count();

                // Clearing the fragmentation status is done so a block does
                // not stay fragmented until it has been evacuated entirely.
                // This ensures we don't keep evacuating objects when this
                // may no longer be needed.
                block.clear_fragmentation_status();

                if holes > 0 {
                    if holes >= MINIMUM_BIN {
                        histograms.marked.increment(
                            holes,
                            block.marked_lines_count() as u32,
                        );
                    }

                    block.recycle();
                }

                amount += 1;

                self.blocks.push(block);
            }
        }

        state.global_allocator.add_blocks(&mut to_release);

        self.reset_current_block();

        amount
    }

    /// Prepares this bucket for a collection.
    pub fn prepare_for_collection(
        &mut self,
        histograms: &mut Histograms,
        evacuate: bool,
    ) {
        let mut required: isize = 0;
        let mut available: isize = 0;

        for block in self.blocks.iter_mut() {
            let holes = block.holes();

            // We ignore blocks with only a single hole, as those are not
            // fragmented and not worth evacuating. This also ensures we ignore
            // blocks added since the last collection, which will have a hole
            // count of 1.
            if evacuate && holes >= MINIMUM_BIN {
                let lines = block.available_lines_count() as u32;

                histograms.available.increment(holes, lines);

                available += lines as isize;
            };

            // We _must_ reset the bytemaps _after_ calculating the above
            // statistics, as those statistics depend on the mark values in
            // these maps.
            block.prepare_for_collection();
        }

        if available > 0 {
            let mut min_bin = 0;
            let mut bin = MAX_HOLES;

            // Bucket 1 refers to blocks with only a single hole. Blocks with
            // just one hole aren't fragmented, so we ignore those here.
            while available > required && bin >= MINIMUM_BIN {
                required += histograms.marked.get(bin) as isize;
                available -= histograms.available.get(bin) as isize;

                min_bin = bin;
                bin -= 1;
            }

            if min_bin > 0 {
                for block in self.blocks.iter_mut() {
                    if block.holes() >= min_bin {
                        block.set_fragmented();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
use std::ops::Drop;

#[cfg(test)]
impl Drop for Bucket {
    fn drop(&mut self) {
        for block in self.blocks.drain() {
            // Dropping the block also finalises it right away.
            drop(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::immix::block::{Block, LINES_PER_BLOCK};
    use crate::immix::bytemap::Bytemap;
    use crate::immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
    use crate::immix::histograms::Histograms;
    use crate::object::Object;
    use crate::object_value;
    use crate::vm::state::State;

    fn global_allocator() -> RcGlobalAllocator {
        GlobalAllocator::with_rc()
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
    fn test_current_block_with_block() {
        let mut bucket = Bucket::new();
        let block = Block::boxed();

        bucket.add_block(block);

        assert!(bucket.current_block().is_some());
    }

    #[test]
    fn test_current_block_without_block() {
        let bucket = Bucket::new();

        assert!(bucket.current_block().is_none());
    }

    #[test]
    fn test_add_block() {
        let mut bucket = Bucket::new();
        let block1 = Block::boxed();
        let block2 = Block::boxed();
        let block1_ptr = DerefPointer::new(&*block1);
        let block2_ptr = DerefPointer::new(&*block2);

        bucket.add_block(block1);

        assert_eq!(bucket.blocks.len(), 1);

        assert!(bucket.current_block == block1_ptr);
        assert!(bucket.current_block == bucket.blocks.head());
        assert!(bucket.current_block.bucket().is_some());

        bucket.add_block(block2);

        assert_eq!(bucket.blocks.len(), 2);

        assert!(bucket.current_block == block2_ptr);
        assert!(bucket.blocks.head() == block1_ptr);
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
            block.free_pointer() == unsafe { block.start_address().offset(1) }
        );

        bucket.allocate(&global_alloc, Object::new(object_value::none()));

        assert!(
            block.free_pointer() == unsafe { block.start_address().offset(2) }
        );
    }

    #[test]
    fn test_allocate_with_recyclable_blocks() {
        let state = State::with_rc(Config::new(), &[]);
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();
        let mut histos = Histograms::new();

        let (_, pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::none()));

        pointer.mark();

        bucket.reclaim_blocks(&state, &mut histos);

        assert_eq!(bucket.blocks.len(), 1);

        let (new_block, new_pointer) = bucket
            .allocate(&global_alloc, Object::new(object_value::float(4.0)));

        assert_eq!(new_block, false);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_float());

        let head = bucket.blocks.head();

        assert!(
            head.free_pointer() == unsafe { head.start_address().offset(5) }
        );
    }

    #[test]
    fn test_reclaim_blocks() {
        let mut bucket = Bucket::new();
        let mut block1 = Block::boxed();
        let block2 = Block::boxed();
        let mut block3 = Block::boxed();
        let state = State::with_rc(Config::new(), &[]);
        let mut histos = Histograms::new();

        block1.used_lines_bytemap.set(LINES_PER_BLOCK - 1);

        block3.used_lines_bytemap.set(2);

        bucket.add_block(block1);
        bucket.add_block(block2);
        bucket.add_block(block3);

        let total = bucket.reclaim_blocks(&state, &mut histos);

        assert_eq!(bucket.blocks.len(), 2);
        assert_eq!(total, 2);

        assert_eq!(bucket.blocks[0].holes(), 1);
        assert_eq!(bucket.blocks[1].holes(), 2);

        assert_eq!(histos.marked.get(1), 0); // Bucket 1 should not be set
        assert_eq!(histos.marked.get(2), 1);
    }

    #[test]
    fn test_reclaim_blocks_full() {
        let mut bucket = Bucket::new();
        let mut block = Block::boxed();
        let mut histos = Histograms::new();
        let state = State::with_rc(Config::new(), &[]);

        for i in 0..LINES_PER_BLOCK {
            block.used_lines_bytemap.set(i);
        }

        bucket.add_block(block);

        let total = bucket.reclaim_blocks(&state, &mut histos);

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(total, 1);
        assert_eq!(bucket.current_block.is_null(), false);
    }

    #[test]
    fn test_prepare_for_collection_without_evacuation() {
        let mut bucket = Bucket::new();
        let mut histos = Histograms::new();

        bucket.add_block(Block::boxed());
        bucket.current_block().unwrap().used_lines_bytemap.set(1);

        bucket.prepare_for_collection(&mut histos, false);

        // No evacuation needed means the available histogram is not updated.
        assert_eq!(histos.available.get(1), 0);

        let block = bucket.current_block().unwrap();

        assert!(block.marked_objects_bytemap.is_empty());
    }

    #[test]
    fn test_prepare_for_collection_with_evacuation() {
        let mut bucket = Bucket::new();
        let block1 = Block::boxed();
        let mut block2 = Block::boxed();
        let mut histos = Histograms::new();

        block2.used_lines_bytemap.set(1);
        block2.used_lines_bytemap.set(3);
        block2.update_hole_count();
        block2.set_fragmented();

        bucket.add_block(block1);
        bucket.add_block(block2);
        histos.marked.increment(2, 1);

        bucket.prepare_for_collection(&mut histos, true);

        assert_eq!(histos.available.get(2), (LINES_PER_BLOCK - 3) as u32);

        let block = bucket.current_block().unwrap();

        assert!(block.is_fragmented());
        assert!(block.marked_objects_bytemap.is_empty());
    }
}
