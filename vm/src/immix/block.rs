//! Immix Blocks
//!
//! Immix blocks are 32 KB of memory containing a number of 128 bytes lines (256
//! to be exact).

use std::ops::Drop;
use std::ptr;
use alloc::heap;

use immix::bitmap::{Bitmap, ObjectMap, LineMap};
use immix::bucket::Bucket;
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

/// The number of objects that can fit in a single line.
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

/// Structure stored in the first line of a block, used to allow objects to
/// retrieve data from the block they belong to.
pub struct BlockHeader {
    pub block: *mut Block,
}

impl BlockHeader {
    pub fn new(block: *mut Block) -> BlockHeader {
        BlockHeader { block: block }
    }

    /// Returns an immutable reference to the block.
    pub fn block(&self) -> &Block {
        unsafe { &*self.block }
    }

    /// Returns a mutable reference to the block.
    pub fn block_mut(&self) -> &mut Block {
        unsafe { &mut *self.block }
    }
}

/// Enum indicating the state of a block.
#[derive(Debug)]
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
    /// 128 bytes of this field are reserved and used for storing a BlockHeader.
    ///
    /// Memory is aligned to 32 KB.
    pub lines: RawObjectPointer,

    /// The status of the block.
    pub status: BlockStatus,

    /// Bitmap used for marking which lines are in use. A line is marked as
    /// in-use by the GC during the mark phase if it contains at least a single
    /// object.
    pub used_lines: LineMap,

    /// Bitmap used for tracking which object slots are live.
    pub mark_bitmap: ObjectMap,

    /// Bitmap used to track which object slots are in use.
    pub used_slots: ObjectMap,

    /// The pointer to use for allocating a new object.
    pub free_pointer: RawObjectPointer,

    /// Pointer marking the end of the free pointer. Objects may not be
    /// allocated into or beyond this pointer.
    pub end_pointer: RawObjectPointer,

    /// Pointer to the bucket that manages this block.
    pub bucket: *mut Bucket,
}

unsafe impl Send for Block {}
unsafe impl Sync for Block {}

impl Block {
    pub fn new() -> Box<Block> {
        let lines =
            unsafe { heap::allocate(BLOCK_SIZE, BLOCK_SIZE) as RawObjectPointer };

        if lines.is_null() {
            panic!("Failed to allocate memory for a new Block");
        }

        let mut block = Box::new(Block {
            lines: lines,
            status: BlockStatus::Available,
            used_lines: LineMap::new(),
            mark_bitmap: ObjectMap::new(),
            used_slots: ObjectMap::new(),
            free_pointer: ptr::null::<Object>() as RawObjectPointer,
            end_pointer: ptr::null::<Object>() as RawObjectPointer,
            bucket: ptr::null::<Bucket>() as *mut Bucket,
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

    /// Returns an immutable reference to the bucket of this block.
    pub fn bucket(&self) -> Option<&Bucket> {
        if self.bucket.is_null() {
            None
        } else {
            Some(unsafe { &*self.bucket })
        }
    }

    /// Sets the bucket of this block.
    pub fn set_bucket(&mut self, bucket: *mut Bucket) {
        self.bucket = bucket;
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

        self.free_pointer = unsafe { self.free_pointer.offset(1) };

        self.used_slots.set(obj_pointer.mark_bitmap_index());

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
        let first_line = self.lines as usize;
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
        self.mark_bitmap.reset();

        // Destruct all objects still in the block.
        for index in OBJECT_START_SLOT..OBJECTS_PER_BLOCK {
            if self.used_slots.is_set(index) {
                unsafe {
                    let pointer = self.lines.offset(index as isize);
                    let mut object = &mut *pointer;

                    object.deallocate_pointers();

                    ptr::drop_in_place(pointer);
                }
            }
        }

        // Wipe the memory so we don't leave any bogus data behind.
        unsafe {
            let ptr = self.lines.offset(OBJECT_START_SLOT as isize);
            let len = OBJECTS_PER_BLOCK - OBJECT_START_SLOT;

            ptr::write_bytes(ptr, 0, len);
        }

        self.status = BlockStatus::Available;

        self.free_pointer = self.start_address();
        self.end_pointer = self.end_address();
        self.bucket = ptr::null::<Bucket>() as *mut Bucket;

        self.used_lines.reset();
        self.used_slots.reset();
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        self.reset();
    }
}
