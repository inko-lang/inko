//! A bump allocator for fied-size objects, based on the Immix allocator.
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::mem::size_of;
use std::sync::atomic::{AtomicU8, Ordering};

/// The size of each block.
const BLOCK_SIZE: usize = 64 * 1024;

/// The size of a single line.
const LINE_SIZE: usize = 256;

/// The number of lines in a block.
const LINES_PER_BLOCK: usize = BLOCK_SIZE / LINE_SIZE;

/// The first line objects can be allocated into.
const FIRST_LINE: usize = size_of::<BlockHeader>() / LINE_SIZE;

/// The bitmask to apply to a pointer to get the address of its block header.
const HEADER_MASK: isize = !(BLOCK_SIZE as isize - 1);

/// The bitmask to apply to a pointer to get the address of its line.
const LINE_MASK: isize = !(LINE_SIZE as isize - 1);

#[inline(always)]
pub(crate) fn free(pointer: *mut u8) {
    let header = unsafe { BlockHeader::for_pointer(pointer) };
    let idx = header.block().line_index_for_pointer(pointer);

    // Safety: the index is always within bounds.
    unsafe {
        header.increment_reusable(idx);
    }
}

/// The header of each block.
///
/// The alignment of this type _must_ equal the line size.
#[repr(align(256))]
struct BlockHeader {
    /// A map that tracks the number of reusable objects per line.
    reusable_objects: [AtomicU8; LINES_PER_BLOCK],

    /// A pointer to the block that owns this header.
    block: *const Block,

    /// The number of values that fit in a single line.
    values_per_line: u8,
}

impl BlockHeader {
    fn init(&mut self, block: *const Block, size: usize) {
        self.block = block as *const _;
        self.values_per_line = (LINE_SIZE / size) as u8;
    }
}

impl BlockHeader {
    unsafe fn for_pointer<'a>(pointer: *mut u8) -> &'a BlockHeader {
        let addr = (pointer as isize & HEADER_MASK) as usize;

        &*(addr as *const BlockHeader)
    }

    #[inline(always)]
    unsafe fn line_available(&self, index: usize) -> bool {
        // If the line count is indeed the maximum number of values per line,
        // then this comparison succeeds and sets the counter to zero, otherwise
        // it leaves it as-is.
        self.reusable_objects
            .get_unchecked(index)
            .compare_exchange(
                self.values_per_line,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    #[inline(always)]
    unsafe fn increment_reusable(&self, index: usize) {
        self.reusable_objects
            .get_unchecked(index)
            .fetch_add(1, Ordering::AcqRel);
    }

    #[inline(always)]
    fn first_reusable_line(&self) -> Option<usize> {
        for (i, n) in self.reusable_objects[FIRST_LINE..].iter().enumerate() {
            if n.load(Ordering::Acquire) == self.values_per_line {
                return Some(i + FIRST_LINE);
            }
        }

        None
    }

    #[inline(always)]
    fn block(&self) -> &Block {
        // Safety: we always write a valid pointer to this field.
        unsafe { &*self.block }
    }
}

/// A block to allocate into.
pub(crate) struct Block {
    /// The memory/lines of this block.
    ///
    /// The first few lines are reserved for the block header.
    lines: *mut u8,

    /// The upper bound of the memory space to bump allocate into.
    upper: *mut u8,

    /// The offset we're currently allocating into.
    current: *mut u8,

    /// The next block in the list.
    next: Option<Box<Block>>,
}

impl Block {
    fn lines_layout() -> Layout {
        // Safety: the block size and alignment is always valid.
        //
        // The size and alignment _must_ be equal such that given a pointer P to
        // the lines, we can mask P to get the corresponding block header.
        unsafe { Layout::from_size_align_unchecked(BLOCK_SIZE, BLOCK_SIZE) }
    }

    pub(crate) fn new(size: usize) -> Box<Block> {
        debug_assert_eq!(
            size & (size - 1),
            0,
            "the size {} must be a power of two",
            size
        );

        // We zero the block so that the various counters of the block header
        // all default to zero, instead of some random garbage value.
        let layout = Block::lines_layout();
        let lines = unsafe { alloc_zeroed(layout) };

        if lines.is_null() {
            handle_alloc_error(layout);
        }

        let mut block = Box::new(Block {
            upper: unsafe { lines.add(BLOCK_SIZE) },
            current: unsafe { lines.add(FIRST_LINE * LINE_SIZE) },
            lines,
            next: None,
        });

        let block_ptr = &*block as *const _;

        block.header_mut().init(block_ptr, size);
        block
    }

    #[inline(always)]
    pub(crate) fn allocate(&mut self, size: usize) -> Option<*mut u8> {
        loop {
            let ptr = self.current;
            let new = unsafe { ptr.add(size) };

            if new > self.upper {
                // If we reach the end of the block then there's no point in
                // finding another hole.
                if self.upper == self.end_address() {
                    return None;
                }

                let idx = self.line_index_for_pointer(new);

                if self.find_next_hole_starting_at(idx) {
                    continue;
                }

                return None;
            }

            self.current = new;
            return Some(ptr);
        }
    }

    #[inline(always)]
    pub(crate) fn find_first_hole(&mut self) -> bool {
        self.find_next_hole_starting_at(FIRST_LINE)
    }

    #[inline(always)]
    pub(crate) fn find_next_hole_starting_at(
        &mut self,
        mut line: usize,
    ) -> bool {
        let mut found = false;
        let mut start = self.end_address();
        let mut stop = self.end_address();

        // Find the start of the next hole.
        while line < LINES_PER_BLOCK {
            // Safety: the line index is always within bounds.
            if unsafe { self.header().line_available(line) } {
                found = true;
                start = self.pointer_for_line(line);
                break;
            } else {
                line += 1;
            }
        }

        // Increment so the next loop doesn't perform its first iteration using
        // a line we already processed.
        line += 1;

        // Find the end of the next hole.
        while line < LINES_PER_BLOCK {
            // Safety: the line index is always within bounds.
            if unsafe { self.header().line_available(line) } {
                line += 1;
            } else {
                stop = self.pointer_for_line(line);
                break;
            }
        }

        self.current = start;
        self.upper = stop;
        found
    }

    #[inline(always)]
    fn header(&self) -> &BlockHeader {
        // Safety: a header is always written to the start of each block.
        unsafe { &*(self.lines as *const BlockHeader) }
    }

    #[inline(always)]
    fn header_mut(&mut self) -> &mut BlockHeader {
        // Safety: a header is always written to the start of each block.
        unsafe { &mut *(self.lines as *mut BlockHeader) }
    }

    #[inline(always)]
    fn line_index_for_pointer(&self, pointer: *mut u8) -> usize {
        let first_line = self.lines as usize;
        let line_addr = (pointer as isize & LINE_MASK) as usize;
        let index = (line_addr - first_line) / LINE_SIZE;

        debug_assert!(
            index >= FIRST_LINE && index < LINES_PER_BLOCK,
            "index {} is not in range {}..{}",
            index,
            FIRST_LINE,
            LINES_PER_BLOCK
        );
        index
    }

    #[inline(always)]
    fn pointer_for_line(&self, line: usize) -> *mut u8 {
        unsafe { self.lines.add(LINE_SIZE * line) }
    }

    #[inline(always)]
    fn end_address(&self) -> *mut u8 {
        unsafe { self.lines.add(BLOCK_SIZE) }
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        // Safety: the memory in `lines` is always initialized at this point.
        unsafe {
            dealloc(self.lines, Block::lines_layout());
        }
    }
}

/// A bump allocator for objects of a fixed size.
pub(crate) struct BumpAllocator {
    /// The size of each value to allocate.
    ///
    /// The size must be a power of two.
    size: usize,

    /// The list of blocks we've consumed.
    ///
    /// The list is a linked list of blocks, starting with the oldest block.
    head: Box<Block>,

    /// The block we're currently allocating into.
    tail: *mut Block,
}

impl BumpAllocator {
    pub(crate) fn new_classes() -> [BumpAllocator; 4] {
        [
            BumpAllocator::new(16),
            BumpAllocator::new(32),
            BumpAllocator::new(64),
            BumpAllocator::new(128),
        ]
    }

    pub(crate) fn new(size: usize) -> BumpAllocator {
        let mut head = Block::new(size);
        let tail = (&mut *head) as *mut _;

        BumpAllocator { size, head, tail }
    }

    #[inline(always)]
    pub(crate) fn allocate(&mut self) -> *mut u8 {
        let size = self.size;

        if let Some(ptr) = self.current().allocate(size) {
            return ptr;
        }

        if self.find_next_block() || self.find_reusable_block() {
            return self.current().allocate(size).unwrap();
        }

        let mut new_blk = Block::new(size);
        let new_ptr = (&mut *new_blk) as *mut _;

        self.current().next = Some(new_blk);
        self.tail = new_ptr;

        // We assume that `size` always fits in an empty block.
        self.current().allocate(size).unwrap()
    }

    #[inline(always)]
    fn find_next_block(&mut self) -> bool {
        while let Some(next) = self.current().next.as_deref_mut() {
            self.tail = next as *mut _;

            if self.current().find_first_hole() {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    fn find_reusable_block(&mut self) -> bool {
        let mut current = Some(self.head.as_mut());

        while let Some(block) = current {
            if let Some(line) = block.header().first_reusable_line() {
                self.tail = block as *mut _;
                return block.find_next_hole_starting_at(line);
            }

            current = block.next.as_deref_mut();
        }

        false
    }

    #[inline(always)]
    fn current(&mut self) -> &mut Block {
        unsafe { &mut *self.tail }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_allocate() {
        let mut block = Block::new(128 / 8);
        let ptr1 = block.allocate(8);
        let ptr2 = block.allocate(8);

        block.current = block.upper;

        let ptr3 = block.allocate(8);

        assert_eq!(
            ptr1,
            Some(unsafe { block.lines.add(FIRST_LINE * LINE_SIZE) })
        );
        assert_eq!(
            ptr2,
            Some(unsafe { block.lines.add((FIRST_LINE * LINE_SIZE) + 8) })
        );
        assert_eq!(ptr3, None);
    }

    #[test]
    fn test_block_line_index_for_pointer() {
        let mut block = Block::new(128 / 8);
        let ptr1 = block.allocate(8).unwrap();
        let ptr2 = block.allocate(8).unwrap();
        let ptr3 = unsafe { block.end_address().sub(8) };

        assert_eq!(block.line_index_for_pointer(ptr1), FIRST_LINE);
        assert_eq!(block.line_index_for_pointer(ptr2), FIRST_LINE);
        assert_eq!(block.line_index_for_pointer(ptr3), LINES_PER_BLOCK - 1);
    }

    #[test]
    fn test_block_find_next_hole() {
        let mut block = Block::new(64);

        unsafe {
            block.header_mut().increment_reusable(FIRST_LINE + 1);
            block.header_mut().increment_reusable(FIRST_LINE + 1);
        }

        assert!(block.find_next_hole_starting_at(FIRST_LINE));

        assert_eq!(block.current, unsafe {
            block.lines.add((FIRST_LINE + 1) * LINE_SIZE)
        });
        assert_eq!(block.upper, unsafe { block.current.add(LINE_SIZE) });

        let ptr = block.allocate(8).unwrap();

        assert_eq!(block.line_index_for_pointer(ptr), FIRST_LINE + 1);
    }
}
