//! VM instruction handlers for error operations.
use vm::instruction::Instruction;
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
                instruction: &Instruction) {
    let register = instruction.arg(0);
    let ptr = process.get_register(instruction.arg(1));

    let result = if ptr.error_value().is_ok() {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);
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
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let error_ptr = process.get_register(instruction.arg(1));

    let integer = error_ptr.error_value().unwrap() as i64;
    let result = ObjectPointer::integer(integer);

    process.set_register(register, result);
}
