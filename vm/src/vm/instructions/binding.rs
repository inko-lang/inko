//! VM instruction handlers for binding operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Gets the Binding of the current scope and sets it in a register
///
/// This instruction requires only one argument: the register to store the
/// object in.
pub fn get_binding(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let binding = process.binding();

    let obj = process.allocate(object_value::binding(binding),
                               machine.state.binding_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}
