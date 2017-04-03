//! VM instruction handlers for reading from STDIN.
use std::io::{self, Read};

use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Reads all the data from STDIN.
///
/// This instruction requires only one argument:
///
/// 1. The register to store the resulting object in.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
#[inline(always)]
pub fn stdin_read(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let mut buffer = String::new();

    let obj = match io::stdin().read_to_string(&mut buffer) {
        Ok(_) => {
            process.allocate(object_value::string(buffer),
                             machine.state.string_prototype)
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}

/// Reads a given number of bytes from STDIN.
///
/// This instruction takes 2 arguments:
///
/// 1. The register to store the resulting object in.
/// 1. The register containing the number of bytes to read, as a positive
///    integer.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
#[inline(always)]
pub fn stdin_read_exact(machine: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let size_ptr = process.get_register(instruction.arg(1));

    let size = size_ptr.integer_value().unwrap() as usize;
    let mut buffer = String::with_capacity(size);
    let stdin = io::stdin();

    let obj = match stdin.take(size as u64).read_to_string(&mut buffer) {
        Ok(_) => {
            process.allocate(object_value::string(buffer),
                             machine.state.string_prototype)
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}

/// Reads an entire line from STDIN into a string.
///
/// This instruction requires 1 argument: the register to store the
/// resulting object in.
///
/// The result of this instruction is either a string containing the read
/// data, or an error object.
#[inline(always)]
pub fn stdin_read_line(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction) {
    let register = instruction.arg(0);
    let mut buffer = String::new();

    let obj = match io::stdin().read_line(&mut buffer) {
        Ok(_) => {
            process.allocate(object_value::string(buffer),
                             machine.state.string_prototype)
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}
