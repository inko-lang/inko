//! Immix Blocks
//!
//! Immix blocks are 32 KB of memory containing a number of 128 bytes lines (256
//! to be exact).

use std::mem;
use std::ops::Drop;
use std::ptr;

use immix::bitmap::{Bitmap, ObjectMap, LineMap};
use object::Object;
use object_pointer::{RawObjectPointer, ObjectPointer};

/// The number of bytes in a block.
pub const BLOCK_SIZE: usize = 32 * 1024;

/// The number of bytes in single line.
pub const LINE_SIZE: usize = 128;

/// The number of bytes to use for a single object. This **must** equal the
/// output of size_of::<Object>().
pub const BYTES_PER_OBJECT: usize = 32;

/// The number of objects that can fit in a block. This is based on the current
/// size of "Object".
pub const OBJECTS_PER_BLOCK: usize = BLOCK_SIZE / BYTES_PER_OBJECT;

/// THe number of objects that can fit in a single line.
pub const OBJECTS_PER_LINE: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The first slot objects can be allocated into. The first 4 slots (a single
/// line or 128 bytes of memory) are reserved for the mark bitmap.
pub const OBJECT_START_SLOT: usize = LINE_SIZE / BYTES_PER_OBJECT;

/// The offset (in bytes) of the first object in a block.
pub const FIRST_OBJECT_BYTE_OFFSET: usize = OBJECT_START_SLOT * BYTES_PER_OBJECT;

/// The mask to apply to go from a pointer to the mark bitmap's start.
pub const OBJECT_BITMAP_MASK: isize = !(BLOCK_SIZE as isize - 1);

/// The mask to apply to go from a pointer to the line's start.
pub const LINE_BITMAP_MASK: isize = !(LINE_SIZE as isize - 1);

/// Enum indicating the state of a block.
pub enum BlockStatus {
    /// The block is usable (either it's completely or partially free)
    Available,

    /// The block is fragmented and objects need to be evacuated.
    Evacuate,
}

/// Structure representing a single block.
///
/// Allocating these structures will use a little bit more memory than the block
/// size due to the various types used (e.g. the used slots bitmap and the block
/// status).
pub struct Block {
    /// The memory to use for the mark bitmap and allocating objects. The first
    /// 128 bytes of this field are used for the mark bitmap.
    pub lines: RawObjectPointer,

    /// The status of the block.
    pub status: BlockStatus,

    /// Bitmap used for marking which lines are in use. A line is marked as
    /// in-use by the GC during the mark phase if it contains at least a single
    /// object.
    pub used_lines: LineMap,

    /// Bitmap used to track which object slots are in use.
    pub used_slots: ObjectMap,

    /// The pointer to use for allocating a new object.
    pub free_pointer: RawObjectPointer,

    /// Pointer marking the end of the free pointer. Objects may not be
    /// allocated into or beyond this pointer.
    pub end_pointer: RawObjectPointer,
}

unsafe impl Send for Block {}
unsafe impl Sync for Block {}

impl Block {
    pub fn new() -> Block {
        let lines = {
            let mut buf = Vec::with_capacity(OBJECTS_PER_BLOCK);
            let ptr = buf.as_mut_ptr() as RawObjectPointer;

            mem::forget(buf);

            ptr
        };

        // Allocate the bitmap into the first 128 bytes of the block.
        unsafe {
            ptr::write(lines as *mut ObjectMap, ObjectMap::new());
        }

        let mut block = Block {
            lines: lines,
            status: BlockStatus::Available,
            used_lines: LineMap::new(),
            used_slots: ObjectMap::new(),
            free_pointer: ptr::null::<Object>() as RawObjectPointer,
            end_pointer: ptr::null::<Object>() as RawObjectPointer,
        };

        block.free_pointer = block.start_address();
        block.end_pointer = block.end_address();

        block
    }

    /// Returns true if objects can be allocated into this block.
    pub fn is_available(&self) -> bool {
        let available = match self.status {
            BlockStatus::Available => true,
            BlockStatus::Evacuate => false,
        };

        if available {
            !self.used_lines.is_full()
        } else {
            false
        }
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

        self.used_slots.set(obj_pointer.mark_bitmap_index());

        self.free_pointer = unsafe { self.free_pointer.offset(1) };

        obj_pointer
    }

    /// Returns true if we can bump allocate into the current block.
    pub fn can_bump_allocate(&self) -> bool {
        self.free_pointer < self.end_pointer
    }

    /// Moves the free/end pointer to the next available hole if any.
    pub fn find_available_hole(&mut self) {
        if self.free_pointer == self.end_address() {
            // We have already consumed the entire block
            return;
        }

        // Determine the index of the line the current free pointer belongs to.
        // TODO: object pointers will need to re-use this, so this should
        // probably either be a Block function or a separate function.
        let line_addr = (self.free_pointer as isize & LINE_BITMAP_MASK) as usize;
        let first_line = unsafe { self.lines.offset(0) as usize };
        let line_index = (line_addr - first_line) / LINE_SIZE;

        let mut line_pointer = self.free_pointer;

        // Iterate over all lines until we find a completely unused one or run
        // out of lines to process.
        for current_line_index in (line_index + 1)..LINE_SIZE {
            line_pointer =
                unsafe { line_pointer.offset(OBJECTS_PER_LINE as isize) };

            if !self.used_lines.is_set(current_line_index) {
                self.free_pointer = line_pointer;

                self.end_pointer = unsafe {
                    self.free_pointer.offset(OBJECTS_PER_LINE as isize)
                };

                break;
            }
        }
    }

    /// Resets the block to a pristine state.
    pub fn reset(&mut self) {
        let mark_bitmap =
            unsafe { &mut *(self.lines as *mut ObjectMap).offset(0) };

        mark_bitmap.reset();

        // Destruct all objects still in the block.
        for index in OBJECT_START_SLOT..OBJECTS_PER_BLOCK {
            if self.used_slots.is_set(index) {
                unsafe { ptr::drop_in_place(self.lines.offset(index as isize)) };
            }
        }

        // Wipe the memory so we don't leave any bogus data behind.
        unsafe {
            let ptr = self.lines.offset(OBJECT_START_SLOT as isize);
            let len = OBJECTS_PER_BLOCK - OBJECT_START_SLOT;

            ptr::write_bytes(ptr, 0, len);
        }

        self.status = BlockStatus::Available;

        self.used_lines.reset();
        self.used_slots.reset();
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        self.reset();
    }
}
