//! VM instruction handlers for local variable operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a local variable to a given register's value.
///
/// This instruction requires two arguments:
///
/// 1. The local variable index to set.
/// 2. The register containing the object to store in the variable.
pub fn set_local(_: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let local_index = instruction.arg(0)?;
    let object = process.get_register(instruction.arg(1)?)?;

    process.set_local(local_index, object);

    Ok(Action::None)
}

/// Gets a local variable and stores it in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the local's value in.
/// 2. The local variable index to get the value from.
pub fn get_local(_: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let local_index = instruction.arg(1)?;
    let object = process.get_local(local_index)?;

    process.set_register(register, object);

    Ok(Action::None)
}

/// Checks if a local variable exists.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in (true or false).
/// 2. The local variable index to check.
pub fn local_exists(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let register = instruction.arg(0)?;
    let local_index = instruction.arg(1)?;

    let value = if process.local_exists(local_index) {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, value);

    Ok(Action::None)
}

/// Sets a local variable in one of the parent bindings.
///
/// This instruction requires 3 arguments:
///
/// 1. The local variable index to set.
/// 2. The number of parent bindings to traverse in order to find the
///    binding to set the variable in.
/// 3. The register containing the value to set.
pub fn set_parent_local(_: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let index = instruction.arg(0)?;
    let depth = instruction.arg(1)?;
    let value = process.get_register(instruction.arg(2)?)?;

    if let Some(binding) = process.binding().find_parent(depth) {
        binding.set_local(index, value);
    } else {
        return_vm_error!(format!("No binding for depth {}", depth),
                         instruction.line);
    }

    Ok(Action::None)
}

/// Gets a local variable in one of the parent bindings.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the local variable in.
/// 2. The number of parent bindings to traverse in order to find the
///    binding to get the variable from.
/// 3. The local variable index to get.
pub fn get_parent_local(_: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let reg = instruction.arg(0)?;
    let depth = instruction.arg(1)?;
    let index = instruction.arg(2)?;

    let binding = process.binding()
        .find_parent(depth)
        .ok_or_else(|| format!("No binding for depth {}", depth))?;

    let object = binding.get_local(index)?;

    process.set_register(reg, object);

    Ok(Action::None)
}
