//! VM instruction handlers for constant operations.
use immix::copy_object::CopyObject;

use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a constant in a given object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register pointing to the object to store the constant in.
/// 2. The string literal index to use for the name.
/// 3. The register pointing to the object to store.
pub fn set_literal_const(machine: &Machine,
                         process: &RcProcess,
                         code: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let target_ptr = process.get_register(instruction.arg(0)?)?;
    let name_index = instruction.arg(1)?;
    let source_ptr = process.get_register(instruction.arg(2)?)?;
    let name = machine.state.intern(code.string(name_index)?);

    let source = copy_if_permanent!(machine.state.permanent_allocator,
                                    source_ptr,
                                    target_ptr);

    target_ptr.add_constant(&process, name, source);

    Ok(Action::None)
}

/// Sets a constant using a runtime allocated String.
///
/// This instruction takes the same arguments as the "set_literal_const"
/// instruction except the 2nd argument should point to a register
/// containing a String to use for the name.
pub fn set_const(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let target_ptr = process.get_register(instruction.arg(0)?)?;
    let name_ptr = process.get_register(instruction.arg(1)?)?;
    let source_ptr = process.get_register(instruction.arg(2)?)?;

    let name_obj = name_ptr.get();
    let name = machine.state.intern(name_obj.value.as_string()?);

    let source = copy_if_permanent!(machine.state.permanent_allocator,
                                    source_ptr,
                                    target_ptr);

    target_ptr.add_constant(&process, name, source);

    Ok(Action::None)
}

/// Looks up a constant and stores it in a register.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the constant in.
/// 2. The register pointing to an object in which to look for the
///    constant.
/// 3. The string literal index containing the name of the constant.
pub fn get_literal_const(machine: &Machine,
                         process: &RcProcess,
                         code: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0)?;
    let src = process.get_register(instruction.arg(1)?)?;
    let name_index = instruction.arg(2)?;
    let name_str = code.string(name_index)?;
    let name = machine.state.intern(name_str);

    let object = src.get()
        .lookup_constant(&name)
        .ok_or_else(|| constant_error!(instruction.arguments[1], name_str))?;

    process.set_register(register, object);

    Ok(Action::None)
}

/// Looks up a constant using a runtime allocated string.
///
/// This instruction requires the same arguments as the "get_literal_const"
/// instruction except the last argument should point to a register
/// containing a String to use for the name.
pub fn get_const(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let src = process.get_register(instruction.arg(1)?)?;

    let name_ptr = process.get_register(instruction.arg(2)?)?;
    let name_obj = name_ptr.get();
    let name_str = name_obj.value.as_string()?;
    let name = machine.state.intern(name_str);

    let object = src.get()
        .lookup_constant(&name)
        .ok_or_else(|| constant_error!(instruction.arguments[1], name_str))?;

    process.set_register(register, object);

    Ok(Action::None)
}

/// Returns true if a constant exists, false otherwise.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the resulting boolean in.
/// 2. The register containing the source object to check.
/// 3. The string literal index to use as the constant name.
pub fn literal_const_exists(machine: &Machine,
                            process: &RcProcess,
                            code: &RcCompiledCode,
                            instruction: &Instruction)
                            -> InstructionResult {
    let register = instruction.arg(0)?;
    let source = process.get_register(instruction.arg(1)?)?;
    let name_index = instruction.arg(2)?;
    let name = machine.state.intern(code.string(name_index)?);

    if source.get().lookup_constant(&name).is_some() {
        process.set_register(register, machine.state.true_object.clone());
    } else {
        process.set_register(register, machine.state.false_object.clone());
    }

    Ok(Action::None)
}
