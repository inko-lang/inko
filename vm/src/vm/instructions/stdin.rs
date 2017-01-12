//! VM instruction handlers for reading from STDIN.
use std::io::{self, Read};

use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Reads the given amount of bytes into a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the amount of bytes to read.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
pub fn stdin_read(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let proto = machine.state.string_prototype.clone();

    let mut buffer = file_reading_buffer!(instruction, process, 1);

    try_io!(io::stdin().read_to_string(&mut buffer), process, register);

    let obj = process.allocate(object_value::string(buffer), proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Reads an entire line from STDIN into a string.
///
/// This instruction requires 1 argument: the register to store the
/// resulting object in.
///
/// The result of this instruction is either a string containing the read
/// data, or an error object.
pub fn stdin_read_line(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let proto = machine.state.string_prototype.clone();

    let mut buffer = String::new();

    try_io!(io::stdin().read_line(&mut buffer), process, register);

    let obj = process.allocate(object_value::string(buffer), proto);

    process.set_register(register, obj);

    Ok(Action::None)
}
