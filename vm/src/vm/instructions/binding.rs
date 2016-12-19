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

/// Gets the binding of a caller.
///
/// If no binding could be found the current binding is returned instead.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the binding object in.
/// 2. An integer indicating the amount of parents to walk upwards.
pub fn get_binding_of_caller(machine: &Machine,
                             process: &RcProcess,
                             _: &RcCompiledCode,
                             instruction: &Instruction)
                             -> InstructionResult {
    let register = instruction.arg(0)?;
    let depth = instruction.arg(1)?;
    let start_context = process.context();

    let binding = if let Some(context) = start_context.find_parent(depth) {
        context.binding()
    } else {
        start_context.binding()
    };

    let obj = process.allocate(object_value::binding(binding),
                               machine.state.binding_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}
