//! VM instruction handlers for error operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_pointer::ObjectPointer;
use process::RcProcess;

/// Checks if a given object is an error object.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the boolean result in.
/// 2. The register of the object to check.
#[inline(always)]
pub fn is_error(machine: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    let register = instruction.arg(0);
    let obj_ptr = process.get_register(instruction.arg(1));

    let obj = obj_ptr.get();

    let result = if obj.value.is_error() {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);

    Ok(Action::None)
}

/// Converts an error object to an integer.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the integer in.
/// 2. The register containing the error.
#[inline(always)]
pub fn error_to_integer(_: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let register = instruction.arg(0);
    let error_ptr = process.get_register(instruction.arg(1));

    let integer = error_ptr.error_value()? as i64;
    let result = ObjectPointer::integer(integer);

    process.set_register(register, result);

    Ok(Action::None)
}
