//! Immix Blocks
//!
//! Immix blocks are 32 KB of memory containing a number of 128 bytes lines (256
//! to be exact).

use std::ops::Drop;
use std::ptr;
use std::heap::{Alloc, Heap, Layout};

use immix::bitmap::{Bitmap, LineMap, ObjectMap};
use immix::bucket::Bucket;
use immix::finalization_list::FinalizationList;
use object::Object;
use object_pointer::{ObjectPointer, RawObjectPointer};

/// The number of bytes in a block.
pub const BLOCK_SIZE: usize = 32 * 1024;

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
pub const BYTES_PER_OBJECT: usize = 32;

/// The number of objects that can fit in a block. This is based on the current
/// size of "Object".
pub const OBJECTS_PER_BLOCK: usize = BLOCK_SIZE / BYTES_PER_OBJECT;

/// The number of objects that can fit in a single line.
pub const OBJECTS_PER_LINE: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The first slot objects can be allocated into. The first 4 slots (a single
/// line or 128 bytes of memory) are reserved for the mark bitmap.
pub const OBJECT_START_SLOT: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The first line objects can be allocated into.
pub const LINE_START_SLOT: usize = 1;

/// The offset (in bytes) of the first object in a block.
pub const FIRST_OBJECT_BYTE_OFFSET: usize =
    OBJECT_START_SLOT * BYTES_PER_OBJECT;

/// The mask to apply to go from a pointer to the mark bitmap's start.
pub const OBJECT_BITMAP_MASK: isize = !(BLOCK_SIZE as isize - 1);

/// The mask to apply to go from a pointer to the line's start.
pub const LINE_BITMAP_MASK: isize = !(LINE_SIZE as isize - 1);

unsafe fn heap_layout_for_block() -> Layout {
    Layout::from_size_align_unchecked(BLOCK_SIZE, BLOCK_SIZE)
}

/// Structure stored in the first line of a block, used to allow objects to
/// retrieve data from the block they belong to.
pub struct BlockHeader {
    pub block: *mut Block,
}

/// Structure representing a single block.
///
/// Allocating these structures will use a little bit more memory than the block
/// size due to the various types used.
pub struct Block {
    /// The memory to use for the mark bitmap and allocating objects. The first
    /// 128 bytes of this field are reserved and used for storing a BlockHeader.
    ///
    /// Memory is aligned to 32 KB.
    pub lines: RawObjectPointer,

    /// This block is fragmented and objects should be evacuated.
    pub fragmented: bool,

    /// Bitmap used for tracking which object slots are live.
    pub marked_objects_bitmap: ObjectMap,

    /// Bitmap used to track which lines contain one or more reachable objects.
    pub used_lines_bitmap: LineMap,

    /// Bitmap used to track which objects need to be finalized.
    pub finalize_bitmap: ObjectMap,

    /// The pointer to use for allocating a new object.
    pub free_pointer: RawObjectPointer,

    /// Pointer marking the end of the free pointer. Objects may not be
    /// allocated into or beyond this pointer.
    pub end_pointer: RawObjectPointer,

    /// Pointer to the bucket that manages this block.
    pub bucket: *mut Bucket,

    /// The number of holes in this block.
    pub holes: usize,
}

unsafe impl Send for Block {}
unsafe impl Sync for Block {}

impl BlockHeader {
    pub fn new(block: *mut Block) -> BlockHeader {
        BlockHeader { block: block }
    }

    /// Returns an immutable reference to the block.
    #[inline(always)]
    pub fn block(&self) -> &Block {
        unsafe { &*self.block }
    }

    /// Returns a mutable reference to the block.
    #[inline(always)]
    pub fn block_mut(&self) -> &mut Block {
        unsafe { &mut *self.block }
    }
}

impl Block {
    pub fn new() -> Box<Block> {
        let lines = unsafe {
            Heap.alloc(heap_layout_for_block()).unwrap() as RawObjectPointer
        };

        let mut block = Box::new(Block {
            lines: lines,
            fragmented: false,
            marked_objects_bitmap: ObjectMap::new(),
            used_lines_bitmap: LineMap::new(),
            finalize_bitmap: ObjectMap::new(),
            free_pointer: ptr::null::<Object>() as RawObjectPointer,
            end_pointer: ptr::null::<Object>() as RawObjectPointer,
            bucket: ptr::null::<Bucket>() as *mut Bucket,
            holes: 1,
        });

        block.free_pointer = block.start_address();
        block.end_pointer = block.end_address();

        // Store a pointer to the block in the first (reserved) line.
        unsafe {
            let pointer = &mut *block as *mut Block;
            let header = BlockHeader::new(pointer);

            ptr::write(block.lines as *mut BlockHeader, header);
        }

        block
    }

    /// Prepares this block for a garbage collection cycle.
    pub fn prepare_for_collection(&mut self) {
        self.used_lines_bitmap.swap_mark_value();
        self.marked_objects_bitmap.reset();
    }

    pub fn update_line_map(&mut self) {
        self.used_lines_bitmap.reset_previous_marks();
    }

    /// Returns an immutable reference to the bucket of this block.
    #[inline(always)]
    pub fn bucket(&self) -> Option<&Bucket> {
        if self.bucket.is_null() {
            None
        } else {
            Some(unsafe { &*self.bucket })
        }
    }

    /// Returns a mutable reference to the bucket of htis block.
    pub fn bucket_mut(&mut self) -> Option<&mut Bucket> {
        if self.bucket.is_null() {
            None
        } else {
            Some(unsafe { &mut *self.bucket })
        }
    }

    /// Sets the bucket of this block.
    pub fn set_bucket(&mut self, bucket: *mut Bucket) {
        self.bucket = bucket;
    }

    pub fn set_fragmented(&mut self) {
        self.fragmented = true;
    }

    /// Returns true if all lines in this block are available.
    pub fn is_empty(&self) -> bool {
        self.used_lines_bitmap.is_empty()
    }

    /// Returns a pointer to the first address to be used for objects.
    pub fn start_address(&self) -> RawObjectPointer {
        unsafe { self.lines.offset(OBJECT_START_SLOT as isize) }
    }

    /// Returns a pointer to the end of this block.
    ///
    /// Since this pointer points _beyond_ the block no objects should be
    /// allocated into this pointer, instead it should _only_ be used to
    /// determine if another pointer falls within a block or not.
    pub fn end_address(&self) -> RawObjectPointer {
        unsafe { self.lines.offset(OBJECTS_PER_BLOCK as isize) }
    }

    /// Bump allocates an object into the current block.
    pub fn bump_allocate(&mut self, object: Object) -> ObjectPointer {
        unsafe {
            ptr::write(self.free_pointer, object);
        }

        let obj_pointer = ObjectPointer::new(self.free_pointer);

        self.free_pointer = unsafe { self.free_pointer.offset(1) };

        if obj_pointer.is_finalizable() {
            obj_pointer.mark_for_finalization();
        }

        obj_pointer
    }

    /// Returns true if we can bump allocate into the current block.
    pub fn can_bump_allocate(&self) -> bool {
        self.free_pointer < self.end_pointer
    }

    pub fn line_index_of_pointer(&self, pointer: RawObjectPointer) -> usize {
        let first_line = self.lines as usize;
        let line_addr = (pointer as isize & LINE_BITMAP_MASK) as usize;

        (line_addr - first_line) / LINE_SIZE
    }

    pub fn object_index_of_pointer(&self, pointer: RawObjectPointer) -> usize {
        let first_line = self.lines as usize;
        let offset = pointer as usize - first_line;

        offset / BYTES_PER_OBJECT
    }

    /// Recycles the current block
    pub fn recycle(&mut self) {
        self.find_available_hole_starting_at(LINE_START_SLOT);
    }

    /// Moves the free/end pointer to the next available hole if any.
    pub fn find_available_hole(&mut self) {
        if self.free_pointer == self.end_address() {
            return;
        }

        let line_index = self.line_index_of_pointer(self.free_pointer);

        self.find_available_hole_starting_at(line_index);
    }

    /// Resets the block to a pristine state.
    ///
    /// Allocated objects are not released or finalized automatically.
    pub fn reset(&mut self) {
        self.fragmented = false;

        // All lines are empty, thus there's only 1 hole.
        self.holes = 1;

        self.free_pointer = self.start_address();
        self.end_pointer = self.end_address();
        self.bucket = ptr::null::<Bucket>() as *mut Bucket;

        self.reset_mark_bitmaps();
        self.finalize_bitmap.reset();
    }

    pub fn reset_mark_bitmaps(&mut self) {
        self.used_lines_bitmap.reset();
        self.marked_objects_bitmap.reset();
    }

    pub fn push_pointers_to_finalize(
        &mut self,
        pointers: &mut FinalizationList,
    ) {
        for index in OBJECT_START_SLOT..OBJECTS_PER_BLOCK {
            if self.finalize_bitmap.is_set(index)
                && !self.marked_objects_bitmap.is_set(index)
            {
                let mut object =
                    unsafe { &mut *self.lines.offset(index as isize) };

                if let Some(attributes) = object.take_attributes() {
                    pointers.push_attributes(attributes);
                }

                if let Some(value) = object.value.take_option() {
                    pointers.push_value(value);
                }

                self.finalize_bitmap.unset(index);
            }
        }
    }

    /// Finalizes all unmarked objects right away.
    pub fn finalize(&mut self) {
        let mut pointers = FinalizationList::new();

        self.push_pointers_to_finalize(&mut pointers);

        pointers.finalize();
    }

    /// Updates the number of holes in this block.
    pub fn update_hole_count(&mut self) {
        let mut in_hole = false;

        self.holes = 0;

        for index in LINE_START_SLOT..LINES_PER_BLOCK {
            let is_set = self.used_lines_bitmap.is_set(index);

            if in_hole && is_set {
                in_hole = false;
            } else if !in_hole && !is_set {
                in_hole = true;
                self.holes += 1;
            }
        }
    }

    /// Returns the number of marked lines in this block.
    pub fn marked_lines_count(&self) -> usize {
        self.used_lines_bitmap.len()
    }

    /// Returns the number of available lines in this block.
    pub fn available_lines_count(&self) -> usize {
        (LINES_PER_BLOCK - 1) - self.marked_lines_count()
    }

    fn find_available_hole_starting_at(&mut self, index: usize) {
        let mut start_set = false;
        let mut stop_set = false;

        for index in index..LINES_PER_BLOCK {
            if start_set && stop_set {
                break;
            }

            let offset = ((index - 1) * OBJECTS_PER_LINE) as isize;

            // Set the free pointer to the start of a hole.
            if !self.used_lines_bitmap.is_set(index) && !start_set {
                unsafe {
                    self.free_pointer = self.start_address().offset(offset);
                }

                start_set = true;
            }

            // Set the end pointer to the end of the hole.
            if start_set && !stop_set && self.used_lines_bitmap.is_set(index) {
                unsafe {
                    self.end_pointer = self.start_address().offset(offset);
                }

                stop_set = true;
            }
        }

        if !stop_set {
            self.end_pointer = self.end_address();
        }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        unsafe {
            Heap.dealloc(self.lines as *mut u8, heap_layout_for_block());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::bitmap::Bitmap;
    use immix::bucket::Bucket;
    use object::Object;
    use object_value::ObjectValue;

    #[test]
    fn test_block_header_new() {
        let mut block = Block::new();
        let header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block.is_null(), false);
    }

    #[test]
    fn test_block_header_block() {
        let mut block = Block::new();
        let header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block().holes, 1);
    }

    #[test]
    fn test_block_header_block_mut() {
        let mut block = Block::new();
        let header = BlockHeader::new(&mut *block as *mut Block);

        assert_eq!(header.block_mut().holes, 1);
    }

    #[test]
    fn test_block_new() {
        let block = Block::new();

        assert_eq!(block.lines.is_null(), false);
        assert_eq!(block.free_pointer.is_null(), false);
        assert_eq!(block.end_pointer.is_null(), false);
        assert!(block.bucket.is_null());
    }

    #[test]
    fn test_block_prepare_for_collection() {
        let mut block = Block::new();

        block.used_lines_bitmap.set(1);
        block.marked_objects_bitmap.set(1);
        block.prepare_for_collection();

        assert!(block.used_lines_bitmap.is_set(1));
        assert_eq!(block.marked_objects_bitmap.is_set(1), false);
    }

    #[test]
    fn test_block_update_line_map() {
        let mut block = Block::new();

        block.used_lines_bitmap.set(1);
        block.prepare_for_collection();
        block.update_line_map();

        assert!(block.used_lines_bitmap.is_empty());
    }

    #[test]
    fn test_block_bucket_without_bucket() {
        let block = Block::new();

        assert!(block.bucket().is_none());
    }

    #[test]
    fn test_block_bucket_with_bucket() {
        let mut block = Block::new();
        let mut bucket = Bucket::new();

        block.set_bucket(&mut bucket as *mut Bucket);

        assert!(block.bucket().is_some());
    }

    #[test]
    fn test_block_set_fragmented() {
        let mut block = Block::new();

        assert_eq!(block.fragmented, false);

        block.set_fragmented();

        assert!(block.fragmented);
    }

    #[test]
    fn test_block_is_empty() {
        let mut block = Block::new();

        assert!(block.is_empty());

        block.used_lines_bitmap.set(1);

        assert_eq!(block.is_empty(), false);
    }

    #[test]
    fn test_block_start_address() {
        let block = Block::new();

        assert_eq!(block.start_address().is_null(), false);
    }

    #[test]
    fn test_block_end_address() {
        let block = Block::new();

        assert_eq!(block.end_address().is_null(), false);
    }

    #[test]
    fn test_block_bump_allocate() {
        let mut block = Block::new();
        let obj = Object::new(ObjectValue::Float(10.0));

        assert!(block.free_pointer == block.start_address());

        assert!(block.bump_allocate(obj).get().value.is_float());
        assert!(
            block.free_pointer == unsafe { block.start_address().offset(1) }
        );

        block.bump_allocate(Object::new(ObjectValue::None));
        assert!(
            block.free_pointer == unsafe { block.start_address().offset(2) }
        );

        block.bump_allocate(Object::new(ObjectValue::None));
        assert!(
            block.free_pointer == unsafe { block.start_address().offset(3) }
        );

        block.bump_allocate(Object::new(ObjectValue::None));
        assert!(
            block.free_pointer == unsafe { block.start_address().offset(4) }
        );

        block.bump_allocate(Object::new(ObjectValue::None));
        assert!(
            block.free_pointer == unsafe { block.start_address().offset(5) }
        );

        assert!(block.finalize_bitmap.is_set(4));
        assert_eq!(block.finalize_bitmap.is_set(5), false);
    }

    #[test]
    fn test_block_can_bump_allocate() {
        let mut block = Block::new();

        assert!(block.can_bump_allocate());

        block.free_pointer = block.end_pointer;

        assert_eq!(block.can_bump_allocate(), false);
    }

    #[test]
    fn test_block_line_index_of_pointer() {
        let block = Block::new();

        assert_eq!(block.line_index_of_pointer(block.free_pointer), 1);
    }

    #[test]
    fn test_block_object_index_of_pointer() {
        let block = Block::new();

        let ptr1 = block.free_pointer;
        let ptr2 = unsafe { block.free_pointer.offset(1) };

        assert_eq!(block.object_index_of_pointer(ptr1), 4);
        assert_eq!(block.object_index_of_pointer(ptr2), 5);
    }

    #[test]
    fn test_block_recycle() {
        let mut block = Block::new();

        // First line is used
        block.used_lines_bitmap.set(1);
        block.recycle();

        assert!(
            block.free_pointer == unsafe { block.start_address().offset(4) }
        );
        assert!(block.end_pointer == block.end_address());

        // first line is available, followed by a used line
        block.used_lines_bitmap.reset();
        block.used_lines_bitmap.set(2);
        block.recycle();

        assert!(block.free_pointer == block.start_address());
        assert!(
            block.end_pointer == unsafe { block.start_address().offset(4) }
        );
    }

    #[test]
    fn test_block_find_available_hole() {
        let mut block = Block::new();

        let pointer1 = block.bump_allocate(Object::new(ObjectValue::None));

        block.used_lines_bitmap.set(1);
        block.find_available_hole();

        let pointer2 = block.bump_allocate(Object::new(ObjectValue::None));

        block.used_lines_bitmap.set(2);
        block.used_lines_bitmap.set(3);
        block.find_available_hole();

        let pointer3 = block.bump_allocate(Object::new(ObjectValue::None));

        assert_eq!(block.line_index_of_pointer(pointer1.raw.raw), 1);
        assert_eq!(block.line_index_of_pointer(pointer2.raw.raw), 2);
        assert_eq!(block.line_index_of_pointer(pointer3.raw.raw), 4);
    }

    #[test]
    fn test_block_find_available_hole_recycle() {
        let mut block = Block::new();

        block.used_lines_bitmap.set(1);
        block.used_lines_bitmap.set(2);
        block.used_lines_bitmap.reset_previous_marks();

        block.find_available_hole_starting_at(1);

        assert_eq!(block.free_pointer, unsafe {
            block.start_address().offset(8)
        });
    }

    #[test]
    fn test_block_find_available_hole_full_block() {
        let mut block = Block::new();

        block.free_pointer = block.end_pointer;

        // Since the block has been "consumed" this method should not modify the
        // free pointer in any way.
        block.find_available_hole();

        assert!(block.free_pointer == block.end_pointer);
    }

    #[test]
    fn test_block_find_available_hole_pointer_range() {
        let mut block = Block::new();

        block.used_lines_bitmap.set(1);
        block.used_lines_bitmap.set(2);
        block.used_lines_bitmap.set(255);

        block.find_available_hole();

        let start_pointer = unsafe {
            block.start_address().offset(2 * OBJECTS_PER_LINE as isize)
        };

        let end_pointer =
            (block.end_address() as usize - LINE_SIZE) as *mut Object;

        assert!(block.free_pointer == start_pointer);
        assert!(block.end_pointer == end_pointer);
    }

    #[test]
    fn test_block_reset() {
        let mut block = Block::new();
        let mut bucket = Bucket::new();

        block.set_fragmented();
        block.holes = 4;

        block.free_pointer = block.end_address();
        block.end_pointer = block.start_address();
        block.set_bucket(&mut bucket as *mut Bucket);
        block.used_lines_bitmap.set(1);
        block.marked_objects_bitmap.set(1);

        block.reset();

        assert_eq!(block.fragmented, false);
        assert_eq!(block.holes, 1);
        assert!(block.free_pointer == block.start_address());
        assert!(block.end_pointer == block.end_address());
        assert!(block.bucket.is_null());
        assert!(block.used_lines_bitmap.is_empty());
        assert!(block.marked_objects_bitmap.is_empty());
        assert!(block.finalize_bitmap.is_empty());
    }

    #[test]
    fn test_block_finalize() {
        let mut block = Block::new();
        let obj = Object::new(ObjectValue::Float(10.0));

        block.bump_allocate(obj);

        block.finalize();

        assert!(block.finalize_bitmap.is_empty());
    }

    #[test]
    fn test_block_update_hole_count() {
        let mut block = Block::new();

        block.used_lines_bitmap.set(1);
        block.used_lines_bitmap.set(3);
        block.used_lines_bitmap.set(10);

        block.update_hole_count();

        assert_eq!(block.holes, 3);
    }

    #[test]
    fn test_block_marked_lines_count() {
        let mut block = Block::new();

        assert_eq!(block.marked_lines_count(), 0);

        block.used_lines_bitmap.set(1);

        assert_eq!(block.marked_lines_count(), 1);
    }

    #[test]
    fn test_block_available_lines_count() {
        let mut block = Block::new();

        assert_eq!(block.available_lines_count(), 255);

        block.used_lines_bitmap.set(1);

        assert_eq!(block.available_lines_count(), 254);
    }
}
