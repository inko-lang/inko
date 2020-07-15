//! Tables for catching thrown values.
//!
//! A CatchTable is used to track which instruction sequences may catch a value
//! that is being thrown. Whenever the VM finds a match it will jump to a target
//! instruction, set a register, and continue execution.
pub struct CatchEntry {
    /// The start position of the instruction range for which to catch a value.
    pub start: usize,

    /// The end position of the instruction range.
    pub end: usize,

    /// The instruction index to jump to.
    pub jump_to: usize,
}

pub struct CatchTable {
    pub entries: Vec<CatchEntry>,
}

impl CatchEntry {
    pub fn new(start: usize, end: usize, jump_to: usize) -> Self {
        CatchEntry {
            start,
            end,
            jump_to,
        }
    }
}

impl CatchTable {
    pub fn new() -> Self {
        CatchTable {
            entries: Vec::new(),
        }
    }
}
