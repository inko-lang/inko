//! VM instruction handlers for nil objects.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a "nil" object in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
pub fn get_nil(machine: &Machine,
               process: &RcProcess,
               _: &RcCompiledCode,
               instruction: &Instruction)
               -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.nil_object.clone());

    Ok(Action::None)
}
