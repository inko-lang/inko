//! Tables for catching thrown values.
//!
//! A CatchTable is used to track which instruction sequences may catch a value
//! that is being thrown. Whenever the VM finds a match it will jump to a target
//! instruction, set a register, and continue execution.
use std::mem;

/// An enum indicating why a value was being thrown.
#[derive(Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum ThrowReason {
    /// A throw used to return from a closure's surrounding scope.
    Return,

    /// A regular throw.
    Throw,
}

pub struct CatchEntry {
    pub reason: ThrowReason,

    /// The start position of the instruction range for which to catch a value.
    pub start: usize,

    /// The end position of the instruction range.
    pub end: usize,

    /// The instruction index to jump to.
    pub jump_to: usize,

    /// The register to store the caught value in.
    pub register: usize,
}

pub struct CatchTable {
    pub entries: Vec<CatchEntry>,
}

impl ThrowReason {
    pub fn from_u8(value: u8) -> Self {
        unsafe { mem::transmute(value) }
    }
}

impl CatchEntry {
    pub fn new(
        reason: ThrowReason,
        start: usize,
        end: usize,
        jump_to: usize,
        register: usize,
    ) -> Self {
        CatchEntry {
            reason: reason,
            start: start,
            end: end,
            jump_to: jump_to,
            register: register,
        }
    }
}

impl CatchTable {
    pub fn new() -> Self {
        CatchTable { entries: Vec::new() }
    }
}
