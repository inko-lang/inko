//! VM instruction handlers for getting and setting global variables.

use process::RcProcess;
use vm::instruction::Instruction;

/// Sets a global variable to a given register's value.
///
/// This instruction requires two arguments:
///
/// 1. The global variable index to set.
/// 2. The register containing the object to store in the variable.
#[inline(always)]
pub fn set_global(process: &RcProcess, instruction: &Instruction) {
    let index = instruction.arg(0);
    let object = process.get_register(instruction.arg(1));

    process.set_global(index, object);
}

/// Gets a global variable and stores it in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the global's value in.
/// 2. The global variable index to get the value from.
#[inline(always)]
pub fn get_global(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let index = instruction.arg(1);
    let object = process.get_global(index);

    process.set_register(register, object);
}
