//! Immix Blocks
//!
//! Immix blocks are 32 KB of memory containing a number of 128 bytes lines (256
//! to be exact).

use immix::line::{Line, LINE_SIZE};
use object::Object;
use object_pointer::ObjectPointer;

/// The number of bytes in a block.
pub const BLOCK_SIZE: usize = 32 * 1024;

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
/// size due to the various types used (e.g. Vec).
pub struct Block {
    pub lines: Vec<Line>,
    pub status: BlockStatus,
    pub lines_used: usize,
    pub line_count: usize,
}

impl Block {
    pub fn new() -> Block {
        let capacity = BLOCK_SIZE / LINE_SIZE;
        let mut lines = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            lines.push(Line::new());
        }

        Block {
            lines: lines,
            status: BlockStatus::Available,
            lines_used: 0,
            line_count: capacity,
        }
    }

    /// Returns true if objects can be allocated into this block.
    pub fn is_available(&self) -> bool {
        let available = match self.status {
            BlockStatus::Available => true,
            BlockStatus::Evacuate => false,
        };

        if available {
            self.lines_used < self.line_count
        } else {
            false
        }
    }

    /// Allocates an object into the current block.
    pub fn allocate(&mut self, object: Object) -> ObjectPointer {
        for line in self.lines.iter_mut() {
            if line.is_available() {
                let pointer = line.allocate(object);

                if !line.is_available() {
                    self.lines_used += 1;
                }

                return pointer;
            }
        }

        // This should only happen when one tried to call this method without
        // first checking if any space was available. Since this is a bug we're
        // using panic! instead of returning something like a Result.
        panic!("Can't allocate into a block without any available lines");
    }
}
