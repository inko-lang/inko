//! VM instruction handlers for method operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Looks up a method and sets it in the target register.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method in.
/// 2. The register containing the object containing the method.
/// 3. The register containing the method name as a String.
///
/// If a method could not be found the target register will be set to nil
/// instead.
pub fn lookup_method(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0)?;
    let rec_ptr = process.get_register(instruction.arg(1)?)?;
    let name_ptr = process.get_register(instruction.arg(2)?)?;

    let name_obj = name_ptr.get();
    let name = machine.state.intern(name_obj.value.as_string()?);

    let method = rec_ptr.get()
        .lookup_method(&name)
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, method);

    Ok(Action::None)
}

/// Defines a method for an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method object in.
/// 2. The register pointing to a specific object to define the method
///    on.
/// 3. The register containing a String to use as the method name.
/// 4. The register containing the Block to use for the method.
pub fn def_method(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let receiver_ptr = process.get_register(instruction.arg(1)?)?;
    let name_ptr = process.get_register(instruction.arg(2)?)?;
    let block_ptr = process.get_register(instruction.arg(3)?)?;

    let name_obj = name_ptr.get();
    let name = machine.state.intern(name_obj.value.as_string()?);

    let block_obj = block_ptr.get();
    let block = block_obj.value.as_block()?;

    let method = machine.allocate_method(&process, &receiver_ptr, block);

    receiver_ptr.add_method(&process, name.clone(), method);

    process.set_register(register, method);

    Ok(Action::None)
}

/// Checks if an object responds to a message.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in (either true or false)
/// 2. The register containing the object to check.
/// 3. The register containing the name to look up, as a string.
pub fn responds_to(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let source = process.get_register(instruction.arg(1)?)?;

    let name_ptr = process.get_register(instruction.arg(2)?)?;
    let name_obj = name_ptr.get();
    let name = machine.state.intern(name_obj.value.as_string()?);

    let source_obj = source.get();

    let result = if source_obj.responds_to(&name) {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);

    Ok(Action::None)
}

/// Removes a method from an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the removed method in.
/// 2. The register containing the object from which to remove the method.
/// 3. The register containing the method name as a string.
///
/// If the method did not exist the target register is set to nil instead.
pub fn remove_method(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0)?;
    let rec_ptr = process.get_register(instruction.arg(1)?)?;
    let name_ptr = process.get_register(instruction.arg(2)?)?;

    let name_obj = name_ptr.get();
    let name = machine.state.intern(name_obj.value.as_string()?);

    let obj = if let Some(method) = rec_ptr.get_mut().remove_method(&name) {
        method
    } else {
        machine.state.nil_object
    };

    process.set_register(register, obj);

    Ok(Action::None)
}
