//! VM instruction handlers for writing to STDOUT.
use std::io::{self, Write};

use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use errors;
use object_value;
use process::RcProcess;

/// Writes a string to STDOUT and returns the amount of written bytes.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the string to write.
///
/// The result of this instruction is either an integer indicating the
/// amount of bytes written, or an error object.
pub fn stdout_write(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let register = instruction.arg(0)?;
    let arg_ptr = process.get_register(instruction.arg(1)?)?;

    let arg = arg_ptr.get();
    let int_proto = machine.state.integer_prototype.clone();
    let mut stdout = io::stdout();

    let result = try_io!(stdout.write(arg.value.as_string()?.as_bytes()),
                         process,
                         register);

    try_io!(stdout.flush(), process, register);

    let obj = process.allocate(object_value::integer(result as i64), int_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}
