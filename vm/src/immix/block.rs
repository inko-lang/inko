//! Immix Blocks
//!
//! Immix blocks are 32 KB of memory containing a number of 128 bytes lines (256
//! to be exact).

use crate::deref_pointer::DerefPointer;
use crate::immix::block_list::BlockIterator;
use crate::immix::bucket::Bucket;
use crate::immix::bytemap::{Bytemap, LineMap, ObjectMap};
use crate::object::Object;
use crate::object_pointer::{ObjectPointer, RawObjectPointer};
use std::alloc::{self, Layout};
use std::mem;
use std::ops::Drop;
use std::ptr;
use std::sync::atomic::spin_loop_hint;

/// The number of bytes in a block.
pub const BLOCK_SIZE: usize = 8 * 1024;

/// The number of bytes in single line.
pub const LINE_SIZE: usize = 128;

/// The number of lines in a block.
pub const LINES_PER_BLOCK: usize = BLOCK_SIZE / LINE_SIZE;

/// The maximum number of holes a block can have. Consecutive empty lines count
/// as one hole, so the max is half the number of lines (used -> empty -> used,
/// etc).
pub const MAX_HOLES: usize = LINES_PER_BLOCK / 2;

/// The number of bytes to use for a single object. This **must** equal the
/// output of size_of::<Object>().
pub const BYTES_PER_OBJECT: usize = mem::size_of::<Object>();

/// The number of objects that can fit in a block. This is based on the current
/// size of "Object".
pub const OBJECTS_PER_BLOCK: usize = BLOCK_SIZE / BYTES_PER_OBJECT;

/// The number of objects that can fit in a single line.
pub const OBJECTS_PER_LINE: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The first slot objects can be allocated into. The first 4 slots (a single
/// line or 128 bytes of memory) are reserved for the mark bytemap.
pub const OBJECT_START_SLOT: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The first line objects can be allocated into.
pub const LINE_START_SLOT: usize = 1;

/// The offset (in bytes) of the first object in a block.
pub const FIRST_OBJECT_BYTE_OFFSET: usize =
    OBJECT_START_SLOT * BYTES_PER_OBJECT;

/// The mask to apply to go from a pointer to the mark bytemap's start.
pub const OBJECT_BYTEMAP_MASK: isize = !(BLOCK_SIZE as isize - 1);

/// The mask to apply to go from a pointer to the line's start.
pub const LINE_BYTEMAP_MASK: isize = !(LINE_SIZE as isize - 1);

unsafe fn heap_layout_for_block() -> Layout {
    Layout::from_size_align_unchecked(BLOCK_SIZE, BLOCK_SIZE)
}

/// Structure stored in the first line of a block, used to allow objects to
/// retrieve data from the block they belong to.
///
/// Because this structure is stored in the first line its size _must_ be less
/// than or equal to the size of a single line (= 128 bytes). Fields are ordered
/// so the struct takes up as little space as possible.
pub struct BlockHeader {
    /// A pointer to the block this header belongs to.
    pub block: *mut Block,

    /// Pointer to the bucket that manages this block.
    pub bucket: *mut Bucket,

    /// The number of holes in this block.
    pub holes: usize,

    /// The next block in the list this block belongs to.
    pub next: DerefPointer<Block>,

    /// This block is fragmented and objects should be evacuated.
    pub fragmented: bool,
}

impl BlockHeader {
    pub fn new(block: *mut Block) -> BlockHeader {
        BlockHeader {
            block,
            bucket: ptr::null::<Bucket>() as *mut Bucket,
            holes: 1,
            next: DerefPointer::null(),
            fragmented: false,
        }
    }

    /// Returns an immutable reference to the block.
    #[inline(always)]
    pub fn block(&self) -> &Block {
        unsafe { &*self.block }
    }

    /// Returns a mutable reference to the block.
    #[inline(always)]
    pub fn block_mut(&mut self) -> &mut Block {
        unsafe { &mut *self.block }
    }

    pub fn bucket(&self) -> Option<&Bucket> {
        if self.bucket.is_null() {
            None
        } else {
            Some(unsafe { &*self.bucket })
        }
    }

    pub fn bucket_mut(&mut self) -> Option<&mut Bucket> {
        if self.bucket.is_null() {
            None
        } else {
            Some(unsafe { &mut *self.bucket })
        }
    }

    pub fn set_next(&mut self, block: DerefPointer<Block>) {
        self.next = block;
    }

    pub fn reset(&mut self) {
        self.fragmented = false;
        self.holes = 1;
        self.bucket = ptr::null_mut();
    }
}

/// Structure representing a single block.
///
/// Allocating these structures will use a little bit more memory than the block
/// size due to the various types used.
pub struct Block {
    /// The pointer to use for allocating a new object.
    pub free_pointer: DerefPointer<Object>,

    /// Pointer marking the end of the free pointer. Objects may not be
    /// allocated into or beyond this pointer.
    pub end_pointer: DerefPointer<Object>,

    /// The memory to use for the mark bytemap and allocating objects. The first
    /// 128 bytes of this field are reserved and used for storing a BlockHeader.
    ///
    /// Memory is aligned to the block size.
    pub lines: RawObjectPointer,

    /// Bytemap used to track which lines contain one or more reachable objects.
    pub used_lines_bytemap: LineMap,

    /// Bytemap used for tracking which object slots are live.
    pub marked_objects_bytemap: ObjectMap,
}

unsafe impl Send for Block {}
unsafe impl Sync for Block {}

impl Block {
    #[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
    pub fn boxed() -> Box<Block> {
        let layout = unsafe { heap_layout_for_block() };
        let lines = unsafe { alloc::alloc_zeroed(layout) as RawObjectPointer };

        if lines.is_null() {
            alloc::handle_alloc_error(layout);
        }

        let mut block = Box::new(Block {
            lines,
            marked_objects_bytemap: ObjectMap::new(),
            used_lines_bytemap: LineMap::new(),
            free_pointer: DerefPointer::null(),
            end_pointer: DerefPointer::null(),
        });

        block.free_pointer = DerefPointer::from_pointer(block.start_address());
        block.end_pointer = DerefPointer::from_pointer(block.end_address());

        // Store a pointer to the block in the first (reserved) line.
        unsafe {
            let pointer = &mut *block as *mut Block;
            let header = BlockHeader::new(pointer);

            ptr::write(block.lines as *mut BlockHeader, header);
        }

        block
    }

    /// Returns an immutable reference to the header of this block.
    #[inline(always)]
    pub fn header(&self) -> &BlockHeader {
        unsafe {
            let pointer = self.lines.offset(0) as *const BlockHeader;

            &*pointer
        }
    }

    /// Returns a mutable reference to the header of this block.
    #[inline(always)]
    pub fn header_mut(&mut self) -> &mut BlockHeader {
        unsafe {
            let pointer = self.lines.offset(0) as *mut BlockHeader;

            &mut *pointer
        }
    }

    /// Returns an immutable reference to the bucket of this block.
    #[inline(always)]
    pub fn bucket(&self) -> Option<&Bucket> {
        self.header().bucket()
    }

    /// Returns a mutable reference to the bucket of htis block.
    #[inline(always)]
    pub fn bucket_mut(&mut self) -> Option<&mut Bucket> {
        self.header_mut().bucket_mut()
    }

    /// Sets the bucket of this block.
    pub fn set_bucket(&mut self, bucket: *mut Bucket) {
        self.header_mut().bucket = bucket;
    }

    pub fn set_fragmented(&mut self) {
        self.header_mut().fragmented = true;
    }

    pub fn clear_fragmentation_status(&mut self) {
        self.header_mut().fragmented = false;
    }

    pub fn is_fragmented(&self) -> bool {
        self.header().fragmented
    }

    pub fn holes(&self) -> usize {
        self.header().holes
    }

    /// Returns true if all lines in this block are available.
    pub fn is_empty(&self) -> bool {
        self.used_lines_bytemap.is_empty()
    }

    /// Returns a pointer to the first address to be used for objects.
    pub fn start_address(&self) -> RawObjectPointer {
        unsafe { self.lines.add(OBJECT_START_SLOT) }
    }

    /// Returns a pointer to the end of this block.
    ///
    /// Since this pointer points _beyond_ the block no objects should be
    /// allocated into this pointer, instead it should _only_ be used to
    /// determine if another pointer falls within a block or not.
    pub fn end_address(&self) -> RawObjectPointer {
        unsafe { self.lines.add(OBJECTS_PER_BLOCK) }
    }

    /// Atomically loads the current free pointer.
    pub fn free_pointer(&self) -> RawObjectPointer {
        self.free_pointer.atomic_load().pointer
    }

    /// Atomically loads the current end pointer.
    pub fn end_pointer(&self) -> RawObjectPointer {
        self.end_pointer.atomic_load().pointer
    }

    /// Atomically sets the new free pointer.
    pub fn set_free_pointer(&mut self, pointer: RawObjectPointer) {
        self.free_pointer.atomic_store(pointer);
    }

    /// Atomically sets the new end pointer.
    pub fn set_end_pointer(&mut self, pointer: RawObjectPointer) {
        self.end_pointer.atomic_store(pointer);
    }

    /// Requests a new pointer to use for an object.
    ///
    /// This method will return a None if no space is available in the current
    /// block.
    pub fn request_pointer(&mut self) -> Option<RawObjectPointer> {
        loop {
            let current = self.free_pointer();
            let end = self.end_pointer();

            if current == end {
                if current == self.end_address() {
                    return None;
                }

                if self.find_available_hole(current, end) {
                    continue;
                } else {
                    return None;
                }
            }

            if current > end {
                // It is possible for a thread to try and request a pointer
                // while another thread is advancing the cursor to the next
                // hole. Between setting the new free and end pointers, there is
                // a small time frame where the free pointer is greated than the
                // end pointer, because the end pointer is still the old value.
                //
                // For this to happen, the order of operations has to be as
                // follows:
                //
                //     Thread A             | Thread B
                //     request_pointer()    | find_available_hole()
                //     -----------------------------------------------
                //                          | 1. update free pointer
                //     2. load free pointer |
                //     3. load end pointer  |
                //                          | 4. update end pointer
                //
                // When this happens, in thread A the free pointer will be
                // observed as being greater than the end pointer. Since the end
                // pointer will be updated very soon, we can just spin for a
                // little while.
                spin_loop_hint();
                continue;
            }

            let next_ptr = unsafe { current.offset(1) };

            if self.free_pointer.compare_and_swap(current, next_ptr) {
                return Some(current);
            }
        }
    }

    pub fn line_index_of_pointer(&self, pointer: RawObjectPointer) -> usize {
        let first_line = self.lines as usize;
        let line_addr = (pointer as isize & LINE_BYTEMAP_MASK) as usize;

        (line_addr - first_line) / LINE_SIZE
    }

    pub fn object_index_of_pointer(&self, pointer: RawObjectPointer) -> usize {
        let first_line = self.lines as usize;
        let offset = pointer as usize - first_line;

        offset / BYTES_PER_OBJECT
    }

    /// Recycles the current block
    pub fn recycle(&mut self) {
        let start = self.start_address();
        let end = self.end_address();

        // Reset the free and end pointer, then try to find a new hole based on
        // the used lines.
        self.set_free_pointer(start);
        self.set_end_pointer(end);

        self.find_available_hole(start, end);
    }

    /// Resets the block to a pristine state.
    ///
    /// Allocated objects are not released or finalized automatically.
    pub fn reset(&mut self) {
        self.header_mut().reset();

        let start_addr = self.start_address();
        let end_addr = self.end_address();

        self.set_free_pointer(start_addr);
        self.set_end_pointer(end_addr);

        self.used_lines_bytemap.reset();
        self.marked_objects_bytemap.reset();
    }

    pub fn prepare_for_collection(&mut self) {
        self.used_lines_bytemap.swap_mark_value();
        self.marked_objects_bytemap.reset();
    }

    pub fn update_line_map(&mut self) {
        self.used_lines_bytemap.reset_previous_marks();
    }

    /// Finalizes all unmarked objects right away.
    pub fn finalize(&mut self) {
        for index in OBJECT_START_SLOT..OBJECTS_PER_BLOCK {
            let raw_ptr = unsafe { self.lines.add(index) };

            ObjectPointer::new(raw_ptr).finalize();
        }
    }

    /// Updates the number of holes in this block, returning the new number of
    /// holes.
    pub fn update_hole_count(&mut self) -> usize {
        let mut in_hole = false;
        let mut holes = 0;

        for index in LINE_START_SLOT..LINES_PER_BLOCK {
            let is_set = self.used_lines_bytemap.is_set(index);

            if in_hole && is_set {
                in_hole = false;
            } else if !in_hole && !is_set {
                in_hole = true;
                holes += 1;
            }
        }

        self.header_mut().holes = holes;

        holes
    }

    /// Returns the number of marked lines in this block.
    pub fn marked_lines_count(&mut self) -> usize {
        self.used_lines_bytemap.len()
    }

    /// Returns the number of available lines in this block.
    pub fn available_lines_count(&mut self) -> usize {
        (LINES_PER_BLOCK - 1) - self.marked_lines_count()
    }

    /// Returns an iterator over block pointers, starting at the current block.
    pub fn iter(&self) -> BlockIterator {
        BlockIterator::starting_at(self)
    }

    fn find_available_hole(
        &mut self,
        old_free: RawObjectPointer,
        old_end: RawObjectPointer,
    ) -> bool {
        let mut found_hole = false;
        let mut new_free = self.end_address();
        let mut new_end = self.end_address();
        let mut line = self.line_index_of_pointer(old_free);

        // Find the start of the hole.
        while line < LINES_PER_BLOCK {
            if !self.used_lines_bytemap.is_set(line) {
                new_free = self.pointer_for_hole_starting_at_line(line);
                found_hole = true;

                break;
            }

            line += 1;
        }

        // Find the end of the hole.
        while line < LINES_PER_BLOCK {
            if self.used_lines_bytemap.is_set(line) {
                new_end = self.pointer_for_hole_starting_at_line(line);
                break;
            }

            line += 1;
        }

        // We use CAS here so that we don't overwrite changes made by
        // concurrently running threads.
        self.free_pointer.compare_and_swap(old_free, new_free);
        self.end_pointer.compare_and_swap(old_end, new_end);

        found_hole
    }

    fn pointer_for_hole_starting_at_line(
        &self,
        line: usize,
    ) -> RawObjectPointer {
        let offset = ((line - 1) * OBJECTS_PER_LINE) as isize;

        unsafe { self.start_address().offset(offset) }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        self.finalize();

        unsafe {
            alloc::dealloc(self.lines as *mut u8, heap_layout_for_block());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::immix::bucket::Bucket;
    use crate::immix::bytemap::Bytemap;
    use crate::object::Object;
    use crate::object_value::ObjectValue;
    use std::mem;

    macro_rules! find_available_hole {
        ($block: expr) => {
            let free = $block.free_pointer.pointer;
            let end = $block.end_pointer.pointer;

            $block.find_available_hole(free, end)
        };
    }

    #[test]
    fn test_block_header_type_size() {
        // Block headers must be smaller than or equal to the size of a single
        // line.
        assert_eq!(mem::size_of::<BlockHeader>(), 40);
    }

    #[test]
    fn test_block_type_size() {
        assert_eq!(mem::size_of::<Block>(), 352);
    }

    #[test]
    fn test_block_header_new() {
        let mut block = Block::boxed();
        let header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block.is_null(), false);
    }

    #[test]
    fn test_block_header_block() {
        let mut block = Block::boxed();
        let header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block().holes(), 1);
    }

    #[test]
    fn test_block_header_block_mut() {
        let mut block = Block::boxed();
        let mut header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block_mut().holes(), 1);
    }

    #[test]
    fn test_block_new() {
        let block = Block::boxed();

        assert_eq!(block.lines.is_null(), false);
        assert_eq!(block.free_pointer().is_null(), false);
        assert_eq!(block.end_pointer().is_null(), false);
        assert!(block.bucket().is_none());
    }

    #[test]
    fn test_block_update_line_map() {
        let mut block = Block::boxed();

        block.used_lines_bytemap.set(1);
        block.used_lines_bytemap.swap_mark_value();
        block.update_line_map();

        assert_eq!(block.used_lines_bytemap.is_set(1), false);
    }

    #[test]
    fn test_block_bucket_without_bucket() {
        let block = Block::boxed();

        assert!(block.bucket().is_none());
    }

    #[test]
    fn test_block_bucket_with_bucket() {
        let mut block = Block::boxed();
        let mut bucket = Bucket::new();

        block.set_bucket(&mut bucket as *mut Bucket);

        assert!(block.bucket().is_some());
    }

    #[test]
    fn test_block_set_fragmented() {
        let mut block = Block::boxed();

        assert_eq!(block.is_fragmented(), false);

        block.set_fragmented();

        assert!(block.is_fragmented());
    }

    #[test]
    fn test_block_is_empty() {
        let mut block = Block::boxed();

        assert!(block.is_empty());

        block.used_lines_bytemap.set(1);

        assert_eq!(block.is_empty(), false);
    }

    #[test]
    fn test_block_start_address() {
        let block = Block::boxed();

        assert_eq!(block.start_address().is_null(), false);
    }

    #[test]
    fn test_block_end_address() {
        let block = Block::boxed();

        assert_eq!(block.end_address().is_null(), false);
    }

    #[test]
    fn test_block_request_pointer() {
        let mut block = Block::boxed();

        assert!(block.request_pointer().is_some());

        assert_eq!(block.free_pointer(), unsafe {
            block.start_address().offset(1)
        });
    }

    #[test]
    fn test_block_request_pointer_advances_hole() {
        let mut block = Block::boxed();
        let start = block.start_address();

        block.used_lines_bytemap.set(2);

        find_available_hole!(block);

        for _ in 0..4 {
            block.request_pointer();
        }

        assert!(block.request_pointer().is_some());

        assert_eq!(block.free_pointer(), unsafe { start.offset(9) });
    }

    #[test]
    fn test_request_pointer_after_preparing_collection() {
        let mut block = Block::boxed();

        // We simulate a block starting with a hole, followed by an empty line,
        // followed by another hole, that we allocate into. This can happen when
        // a survivor space is allocated into when it still has recyclable
        // blocks, such as during evacuation.
        block.used_lines_bytemap.set(2);
        block.recycle();
        block.prepare_for_collection();

        // At this point line 2 should not be allocated into, because it was
        // still marked as live.
        for i in 0..8 {
            let pointer = ObjectPointer::new(block.request_pointer().unwrap());

            Object::new(ObjectValue::Integer(i)).write_to(pointer.raw.raw);
        }

        for i in 0..4 {
            let raw_pointer = unsafe { block.start_address().add(i) };

            assert!(ObjectPointer::new(raw_pointer).is_integer());
        }

        for i in 4..8 {
            let raw_pointer = unsafe { block.start_address().add(i) };

            assert!(ObjectPointer::new(raw_pointer).get().value.is_none());
        }

        for i in 8..12 {
            let raw_pointer = unsafe { block.start_address().add(i) };

            assert!(ObjectPointer::new(raw_pointer).is_integer());
        }
    }

    #[test]
    fn test_block_request_all_pointers() {
        let mut block = Block::boxed();
        let mut offset = 0;

        while let Some(pointer) = block.request_pointer() {
            let expected = unsafe { block.start_address().offset(offset) };

            assert_eq!(pointer, expected);
            assert_ne!(pointer, block.end_address());

            offset += 1;
        }

        assert_eq!(block.free_pointer(), block.end_address());
    }

    #[test]
    fn test_block_line_index_of_pointer() {
        let block = Block::boxed();

        assert_eq!(block.line_index_of_pointer(block.free_pointer()), 1);
    }

    #[test]
    fn test_block_object_index_of_pointer() {
        let block = Block::boxed();

        let ptr1 = block.free_pointer();
        let ptr2 = unsafe { block.free_pointer().offset(1) };

        assert_eq!(block.object_index_of_pointer(ptr1), 4);
        assert_eq!(block.object_index_of_pointer(ptr2), 5);
    }

    #[test]
    fn test_block_recycle() {
        let mut block = Block::boxed();

        // First line is used
        block.used_lines_bytemap.set(1);
        block.recycle();

        assert_eq!(block.free_pointer(), unsafe {
            block.start_address().offset(4)
        });

        assert_eq!(block.end_pointer(), block.end_address());

        // first line is available, followed by a used line
        block.used_lines_bytemap.reset();
        block.used_lines_bytemap.set(2);
        block.recycle();

        assert_eq!(block.free_pointer(), block.start_address());

        assert_eq!(block.end_pointer(), unsafe {
            block.start_address().offset(4)
        });
    }

    #[test]
    fn test_block_find_available_hole_lines_of_pointers() {
        let mut block = Block::boxed();

        let pointer1 = Object::new(ObjectValue::None)
            .write_to(block.request_pointer().unwrap());

        block.used_lines_bytemap.set(1);

        find_available_hole!(block);

        let pointer2 = Object::new(ObjectValue::None)
            .write_to(block.request_pointer().unwrap());

        block.used_lines_bytemap.set(2);
        block.used_lines_bytemap.set(3);

        find_available_hole!(block);

        let pointer3 = Object::new(ObjectValue::None)
            .write_to(block.request_pointer().unwrap());

        assert_eq!(block.line_index_of_pointer(pointer1.raw.raw), 1);
        assert_eq!(block.line_index_of_pointer(pointer2.raw.raw), 2);
        assert_eq!(block.line_index_of_pointer(pointer3.raw.raw), 4);
    }

    #[test]
    fn test_block_find_available_hole() {
        let mut block = Block::boxed();
        let start = block.start_address();

        block.used_lines_bytemap.set(1);

        find_available_hole!(block);

        assert_eq!(block.free_pointer(), unsafe { start.offset(4) });
        assert_eq!(block.end_pointer(), block.end_address());
    }

    #[test]
    fn test_block_find_available_hole_with_empty_line_between_used_ones() {
        let mut block = Block::boxed();
        let start = block.start_address();

        block.used_lines_bytemap.set(1);
        block.used_lines_bytemap.set(3);

        find_available_hole!(block);

        assert_eq!(block.free_pointer(), unsafe { start.offset(4) });
        assert_eq!(block.end_pointer(), unsafe { start.offset(8) });
    }

    #[test]
    fn test_block_find_available_hole_full_block() {
        let mut block = Block::boxed();

        for index in 1..LINES_PER_BLOCK {
            block.used_lines_bytemap.set(index);
        }

        find_available_hole!(block);

        assert_eq!(block.free_pointer(), block.end_address());
        assert_eq!(block.end_pointer(), block.end_address());
    }

    #[test]
    fn test_block_find_available_hole_recycle() {
        let mut block = Block::boxed();

        block.used_lines_bytemap.set(1);
        block.used_lines_bytemap.set(2);

        find_available_hole!(block);

        assert_eq!(block.free_pointer(), unsafe {
            block.start_address().offset(8)
        });
    }

    #[test]
    fn test_block_find_available_hole_pointer_range() {
        let mut block = Block::boxed();

        block.used_lines_bytemap.set(1);
        block.used_lines_bytemap.set(2);
        block.used_lines_bytemap.set(LINES_PER_BLOCK - 1);

        find_available_hole!(block);

        let start_pointer = unsafe {
            block.start_address().offset(2 * OBJECTS_PER_LINE as isize)
        };

        let end_pointer =
            (block.end_address() as usize - LINE_SIZE) as *mut Object;

        assert!(block.free_pointer() == start_pointer);
        assert!(block.end_pointer() == end_pointer);
    }

    #[test]
    fn test_block_reset() {
        let mut block = Block::boxed();
        let mut bucket = Bucket::new();

        block.set_fragmented();
        block.header_mut().holes = 4;

        let start_addr = block.start_address();
        let end_addr = block.end_address();

        block.set_free_pointer(end_addr);
        block.set_end_pointer(start_addr);

        block.set_bucket(&mut bucket as *mut Bucket);
        block.used_lines_bytemap.set(1);
        block.marked_objects_bytemap.set(1);

        block.reset();

        assert_eq!(block.is_fragmented(), false);
        assert_eq!(block.holes(), 1);
        assert!(block.free_pointer() == block.start_address());
        assert!(block.end_pointer() == block.end_address());
        assert!(block.bucket().is_none());
        assert!(block.used_lines_bytemap.is_empty());
        assert!(block.marked_objects_bytemap.is_empty());
    }

    #[test]
    fn test_block_finalize() {
        let mut block = Block::boxed();
        let raw_pointer = block.request_pointer().unwrap();
        let pointer = ObjectPointer::new(raw_pointer);

        {
            let mut obj = Object::new(ObjectValue::Float(10.0));

            obj.add_attribute(pointer, pointer);
            obj.write_to(raw_pointer);
        }

        block.finalize();

        let obj = pointer.get();

        assert!(obj.attributes.is_null());
        assert!(obj.prototype.is_null());
        assert!(obj.value.is_none());
    }

    #[test]
    fn test_block_update_hole_count() {
        let mut block = Block::boxed();

        block.used_lines_bytemap.set(1);
        block.used_lines_bytemap.set(3);
        block.used_lines_bytemap.set(10);

        block.update_hole_count();

        assert_eq!(block.holes(), 3);
    }

    #[test]
    fn test_block_marked_lines_count() {
        let mut block = Block::boxed();

        assert_eq!(block.marked_lines_count(), 0);

        block.used_lines_bytemap.set(1);

        assert_eq!(block.marked_lines_count(), 1);
    }

    #[test]
    fn test_block_available_lines_count() {
        let mut block = Block::boxed();

        assert_eq!(block.available_lines_count(), LINES_PER_BLOCK - 1);

        block.used_lines_bytemap.set(1);

        assert_eq!(block.available_lines_count(), LINES_PER_BLOCK - 2);
    }

    #[test]
    fn test_clear_fragmentation_status() {
        let mut block = Block::boxed();

        block.set_fragmented();
        block.clear_fragmentation_status();

        assert_eq!(block.is_fragmented(), false);
    }

    #[test]
    fn test_prepare_for_collection() {
        let mut block = Block::boxed();

        block.used_lines_bytemap.set(1);
        block.marked_objects_bytemap.set(1);
        block.prepare_for_collection();

        assert!(block.used_lines_bytemap.is_set(1));
        assert_eq!(block.marked_objects_bytemap.is_set(1), false);
    }
}
