//! Block Buckets
//!
//! A Bucket contains a sequence of Immix blocks that all contain objects of the
//! same age.
use crate::deref_pointer::DerefPointer;
use crate::immix::block::Block;
use crate::immix::block_list::BlockList;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::immix::histograms::Histograms;
use crate::object::Object;
use crate::object_pointer::ObjectPointer;
use crate::vm::state::RcState;
use parking_lot::Mutex;
use rayon::prelude::*;
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

    /// The objects in this bucket should be promoted to the mature generation.
    pub promote: bool,
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
            promote: false,
            lock: UnsafeCell::new(Mutex::new(())),
        }
    }

    pub fn reset_age(&mut self) {
        self.age = 0;
        self.promote = false;
    }

    pub fn increment_age(&mut self) {
        self.age += 1;
    }

    pub fn number_of_blocks(&self) -> u32 {
        // The maximum value of u32 is 4 294 967 295. With every block being 32
        // KB in size, this means we have an upper limit of 128 TB per process.
        // That seems more than enough, and allows us to more efficiently store
        // this number compared to using an u64/usize.
        self.blocks.len() as u32
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
        self.blocks.push_front(block);
    }

    pub fn reset_current_block(&mut self) {
        let new_pointer = if let Some(pointer) = self.blocks.head_mut() {
            DerefPointer::new(pointer)
        } else {
            DerefPointer::null()
        };

        self.set_current_block(new_pointer);
    }

    /// Allocates an object into this bucket
    ///
    /// The return value is a tuple containing a boolean that indicates if a new
    /// block was requested, and the pointer to the allocated object.
    pub fn allocate(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let mut new_block = false;

        loop {
            let mut advance_block = false;
            let started_at = self.current_block.atomic_load();

            if let Some(mut current) = self.current_block() {
                for block in current.iter_mut() {
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
                            self.current_block
                                .compare_and_swap(started_at.pointer, block);
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

    /// Allocates an object for a mutator into this bucket
    ///
    /// The return value is the same as `Bucket::allocate()`.
    ///
    /// This method does not use synchronisation, so it _can not_ be safely used
    /// from a collector thread.
    pub unsafe fn allocate_for_mutator(
        &mut self,
        global_allocator: &RcGlobalAllocator,
        object: Object,
    ) -> (bool, ObjectPointer) {
        let mut new_block = false;

        loop {
            let mut advance_block = false;

            if let Some(mut current) = self.current_block() {
                for block in current.iter_mut() {
                    if block.is_fragmented() {
                        // The block is fragmented, so skip it. The next time we
                        // find an available block we'll set it as the current
                        // block.
                        advance_block = true;

                        continue;
                    }

                    if let Some(raw_pointer) =
                        block.request_pointer_for_mutator()
                    {
                        if advance_block {
                            self.current_block = DerefPointer::new(block);
                        }

                        return (new_block, object.write_to(raw_pointer));
                    }
                }
            }

            new_block = true;
            self.add_block(global_allocator.request_block());
        }
    }

    /// Returns true if this bucket contains blocks that need to be evacuated.
    pub fn should_evacuate(&self) -> bool {
        // The Immix paper states that one should evacuate when there are one or
        // more recyclable or fragmented blocks. In IVM all objects are the same
        // size and thus it's not possible to have any recyclable blocks left by
        // the time we start a collection (as they have all been consumed). As
        // such we don't check for these and instead only check for fragmented
        // blocks.
        self.blocks.iter().any(Block::is_fragmented)
    }

    /// Reclaims the blocks in this bucket
    ///
    /// Recyclable blocks are scheduled for re-use by the allocator, empty
    /// blocks are to be returned to the global pool, and full blocks are kept.
    pub fn reclaim_blocks(&mut self, state: &RcState, histograms: &Histograms) {
        self.blocks
            .pointers()
            .into_par_iter()
            .for_each(|mut block| {
                block.update_line_map();

                if block.is_empty() {
                    block.reset();
                } else {
                    let holes = block.update_hole_count();

                    if holes > 0 {
                        histograms.marked.increment(
                            holes,
                            block.marked_lines_count() as u16,
                        );

                        block.recycle();
                    }
                }
            });

        // We partition the blocks in sequence so we don't need to synchronise
        // access to the destination lists.
        for block in self.blocks.drain() {
            if block.is_empty() {
                state.global_allocator.add_block(block);
            } else {
                self.blocks.push_front(block);
            }
        }

        self.reset_current_block();
    }

    /// Prepares this bucket for a collection.
    ///
    /// Returns true if evacuation is needed for this bucket.
    pub fn prepare_for_collection(&mut self, histograms: &Histograms) -> bool {
        let mut available: isize = 0;
        let mut required: isize = 0;
        let evacuate = self.should_evacuate();

        for block in self.blocks.iter_mut() {
            let holes = block.holes();

            if evacuate && holes > 0 {
                let count = block.available_lines_count() as u16;

                histograms.available.increment(holes, count);

                available += count as isize;
            }

            block.prepare_for_collection();
        }

        if available > 0 {
            let mut iter = histograms.marked.iter();
            let mut min_bin = None;

            while available > required {
                if let Some(bin) = iter.next() {
                    required += histograms.marked.get(bin) as isize;
                    available -= histograms.available.get(bin) as isize;

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

        bucket.add_block(Block::boxed());

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.current_block.is_null(), false);
        assert!(bucket.blocks[0].bucket().is_some());

        assert!(
            bucket.current_block.pointer as *const Block
                == &*bucket.blocks.head().unwrap() as *const Block
        );

        bucket.add_block(Block::boxed());

        assert_eq!(bucket.blocks.len(), 2);

        assert!(
            bucket.current_block.pointer as *const Block
                == &bucket.blocks[0] as *const Block
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
        let histos = Histograms::new();

        let (_, pointer) =
            bucket.allocate(&global_alloc, Object::new(object_value::none()));

        pointer.mark();

        bucket.reclaim_blocks(&state, &histos);

        assert_eq!(bucket.blocks.len(), 1);

        let (new_block, new_pointer) = bucket
            .allocate(&global_alloc, Object::new(object_value::float(4.0)));

        assert_eq!(new_block, false);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_float());

        let head = bucket.blocks.head().unwrap();

        assert!(
            head.free_pointer() == unsafe { head.start_address().offset(5) }
        );
    }

    #[test]
    fn test_allocate_for_mutator_without_blocks() {
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();

        let (new_block, pointer) = unsafe {
            bucket.allocate_for_mutator(
                &global_alloc,
                Object::new(object_value::none()),
            )
        };

        assert!(new_block);
        assert!(pointer.get().value.is_none());

        let block = pointer.block();

        assert!(
            block.free_pointer() == unsafe { block.start_address().offset(1) }
        );

        unsafe {
            bucket.allocate_for_mutator(
                &global_alloc,
                Object::new(object_value::none()),
            );
        }

        assert!(
            block.free_pointer() == unsafe { block.start_address().offset(2) }
        );
    }

    #[test]
    fn test_allocate_for_mutator_with_recyclable_blocks() {
        let state = State::with_rc(Config::new(), &[]);
        let global_alloc = global_allocator();
        let mut bucket = Bucket::new();
        let histos = Histograms::new();

        let (_, pointer) = unsafe {
            bucket.allocate_for_mutator(
                &global_alloc,
                Object::new(object_value::none()),
            )
        };

        pointer.mark();

        bucket.reclaim_blocks(&state, &histos);

        assert_eq!(bucket.blocks.len(), 1);

        let (new_block, new_pointer) = unsafe {
            bucket.allocate_for_mutator(
                &global_alloc,
                Object::new(object_value::float(4.0)),
            )
        };

        assert_eq!(new_block, false);
        assert!(pointer.get().value.is_none());
        assert!(new_pointer.get().value.is_float());

        let head = bucket.blocks.head().unwrap();

        assert!(
            head.free_pointer() == unsafe { head.start_address().offset(5) }
        );
    }

    #[test]
    fn test_should_evacuate_with_fragmented_blocks() {
        let mut bucket = Bucket::new();
        let mut block = Block::boxed();

        block.set_fragmented();

        bucket.add_block(block);

        assert!(bucket.should_evacuate());
    }

    #[test]
    fn test_reclaim_blocks() {
        let mut bucket = Bucket::new();
        let mut block1 = Block::boxed();
        let block2 = Block::boxed();
        let mut block3 = Block::boxed();
        let state = State::with_rc(Config::new(), &[]);
        let histos = Histograms::new();

        block1.used_lines_bitmap.set(LINES_PER_BLOCK - 1);
        block3.used_lines_bitmap.set(2);

        bucket.add_block(block1);
        bucket.add_block(block2);
        bucket.add_block(block3);
        bucket.reclaim_blocks(&state, &histos);

        assert_eq!(bucket.blocks.len(), 2);

        assert_eq!(bucket.blocks[0].holes(), 1);
        assert_eq!(bucket.blocks[1].holes(), 2);

        assert_eq!(histos.marked.get(1), 1);
        assert_eq!(histos.marked.get(2), 1);
    }

    #[test]
    fn test_reclaim_blocks_full() {
        let mut bucket = Bucket::new();
        let mut block = Block::boxed();
        let histos = Histograms::new();
        let state = State::with_rc(Config::new(), &[]);

        for i in 0..LINES_PER_BLOCK {
            block.used_lines_bitmap.set(i);
        }

        bucket.add_block(block);
        bucket.reclaim_blocks(&state, &histos);

        assert_eq!(bucket.blocks.len(), 1);
        assert_eq!(bucket.current_block.is_null(), false);
    }

    #[test]
    fn test_prepare_for_collection_without_evacuation() {
        let mut bucket = Bucket::new();
        let histos = Histograms::new();

        bucket.add_block(Block::boxed());
        bucket.current_block().unwrap().used_lines_bitmap.set(1);

        assert_eq!(bucket.prepare_for_collection(&histos), false);

        // No evacuation needed means the available histogram is not updated.
        assert_eq!(histos.available.get(1), 0);

        let block = bucket.current_block().unwrap();

        assert!(block.marked_objects_bitmap.is_empty());
    }

    #[test]
    fn test_prepare_for_collection_with_evacuation() {
        let mut bucket = Bucket::new();
        let mut block1 = Block::boxed();
        let histos = Histograms::new();
        let block2 = Block::boxed();

        block1.used_lines_bitmap.set(1);
        block1.set_fragmented();

        bucket.add_block(block1);
        bucket.add_block(block2);

        // Normally the collector updates the mark histogram at the end of a
        // cycle. Since said code is not executed by the function we're testing
        // we'll update this histogram manually.
        histos.marked.increment(1, 1);

        assert!(bucket.prepare_for_collection(&histos));

        assert_eq!(
            histos.available.get(1),
            // We added two blocks worth of lines. Line 0 in every block is
            // reserved, meaning we have to subtract two lines from the number
            // of available ones. We also marked line 1 of block 1 as in use, so
            // we have to subtract another line.
            ((LINES_PER_BLOCK * 2) - 3) as u16
        );

        let block = bucket.current_block().unwrap();

        assert!(block.is_fragmented());
        assert!(block.marked_objects_bitmap.is_empty());
    }
}
