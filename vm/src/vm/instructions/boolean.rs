//! VM instruction handlers for boolean operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a "true" value in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_true(machine: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.true_object.clone());

    Ok(Action::None)
}

/// Sets a "false" value in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_false(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.false_object.clone());

    Ok(Action::None)
}
