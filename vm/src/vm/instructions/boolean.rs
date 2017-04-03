//! VM instruction handlers for boolean operations.
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Sets a "true" value in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_true(machine: &Machine,
                process: &RcProcess,
                instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.true_object);
}

/// Sets a "false" value in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_false(machine: &Machine,
                 process: &RcProcess,
                 instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.false_object);
}
