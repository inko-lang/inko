//! VM instruction handlers for getting/setting object prototypes.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets the prototype of an object.
///
/// This instruction requires two arguments:
///
/// 1. The register containing the object for which to set the prototype.
/// 2. The register containing the object to use as the prototype.
pub fn set_prototype(_: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let source = process.get_register(instruction.arg(0)?)?;
    let proto = process.get_register(instruction.arg(1)?)?;

    source.get_mut().set_prototype(proto);

    Ok(Action::None)
}

/// Gets the prototype of an object.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the prototype in.
/// 2. The register containing the object to get the prototype from.
pub fn get_prototype(_: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0)?;
    let source = process.get_register(instruction.arg(1)?)?;

    let source_obj = source.get();

    let proto = source_obj.prototype()
        .ok_or_else(|| {
            format!("The object in register {} does not have a prototype",
                    instruction.arguments[1])
        })?;

    process.set_register(register, proto);

    Ok(Action::None)
}

/// Returns the prototype to use for integer objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_integer_prototype(machine: &Machine,
                             process: &RcProcess,
                             _: &RcCompiledCode,
                             instruction: &Instruction)
                             -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.integer_prototype.clone());

    Ok(Action::None)
}

/// Returns the prototype to use for float objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_float_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.float_prototype.clone());

    Ok(Action::None)
}

/// Returns the prototype to use for string objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_string_prototype(machine: &Machine,
                            process: &RcProcess,
                            _: &RcCompiledCode,
                            instruction: &Instruction)
                            -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.string_prototype.clone());

    Ok(Action::None)
}

/// Returns the prototype to use for array objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_array_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.array_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for true objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_true_prototype(machine: &Machine,
                          process: &RcProcess,
                          _: &RcCompiledCode,
                          instruction: &Instruction)
                          -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.true_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for false objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_false_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.false_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for method objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_method_prototype(machine: &Machine,
                            process: &RcProcess,
                            _: &RcCompiledCode,
                            instruction: &Instruction)
                            -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.method_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for Binding objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_binding_prototype(machine: &Machine,
                             process: &RcProcess,
                             _: &RcCompiledCode,
                             instruction: &Instruction)
                             -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.binding_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for compiled code objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
pub fn get_compiled_code_prototype(machine: &Machine,
                                   process: &RcProcess,
                                   _: &RcCompiledCode,
                                   instruction: &Instruction)
                                   -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.compiled_code_prototype.clone());

    Ok(Action::None)
}

/// Gets the prototype to use for "nil" objects.
///
/// This instruction requires one argument: the register to store the prototype
/// in.
pub fn get_nil_prototype(machine: &Machine,
                         process: &RcProcess,
                         _: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0)?;

    process.set_register(register, machine.state.nil_prototype.clone());

    Ok(Action::None)
}
