//! VM instruction handlers for writing to STDERR.
use std::io::{self, Write};

use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_pointer::ObjectPointer;
use process::RcProcess;

/// Writes a string to STDERR and returns the amount of written bytes.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the string to write.
///
/// The result of this instruction is either an integer indicating the
/// amount of bytes written, or an error object.
#[inline(always)]
pub fn stderr_write(_: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction) {
    let register = instruction.arg(0);
    let string_ptr = process.get_register(instruction.arg(1));
    let string = string_ptr.string_value().unwrap();
    let mut stderr = io::stderr();

    let obj = match stderr.write(string.as_bytes()) {
        Ok(num_bytes) => {
            match stderr.flush() {
                Ok(_) => ObjectPointer::integer(num_bytes as i64),
                Err(error) => io_error_code!(process, error),
            }
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}
