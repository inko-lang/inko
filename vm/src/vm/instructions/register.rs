//! VM instruction handlers for working directly with registers.
use process::RcProcess;
use vm::instruction::Instruction;

/// Sets a register to the value of another register.
///
/// This instruction requires two arguments:
///
/// 1. The register to set.
/// 2. The register to get the value from.
#[inline(always)]
pub fn set_register(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let value = process.get_register(instruction.arg(1));

    process.set_register(register, value);
}
