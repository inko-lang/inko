//! Result types for instruction handlers.
use vm::action::Action;

/// The Result type produced by an instruction.
pub type InstructionResult = Result<Action, String>;
