//! VM instruction handlers for error operations.
use catch_table::ThrowReason;
use object_pointer::ObjectPointer;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Checks if a given object is an error object.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the boolean result in.
/// 2. The register of the object to check.
#[inline(always)]
pub fn is_error(machine: &Machine,
                process: &RcProcess,
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
pub fn error_to_integer(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let error_ptr = process.get_register(instruction.arg(1));

    let integer = error_ptr.error_value().unwrap() as i64;
    let result = ObjectPointer::integer(integer);

    process.set_register(register, result);
}

/// Throws a value
///
/// This instruction requires two arguments:
///
/// 1. The reason for throwing the value as a value in the ThrowReason enum.
/// 2. The register containing the value to throw.
///
/// This method will unwind the call stack until either the value is caught, or
/// until we reach the top level (at which point we terminate the VM).
#[inline(always)]
pub fn throw(process: &RcProcess, instruction: &Instruction) {
    let reason = ThrowReason::from_u8(instruction.arg(0) as u8);
    let value = process.get_register(instruction.arg(1));

    loop {
        let code = process.compiled_code();
        let mut context = process.context_mut();
        let index = context.instruction_index;

        for entry in code.catch_table.entries.iter() {
            if entry.reason == reason && entry.start < index &&
               entry.end >= index {
                context.instruction_index = entry.jump_to;
                context.set_register(entry.register, value);
                return;
            }
        }

        if process.pop_context() {
            panic!("A thrown value reached the top-level in process {}",
                   process.pid);
        }
    }
}
