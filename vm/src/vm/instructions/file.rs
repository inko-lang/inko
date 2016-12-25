//! VM instruction handlers for working with files.
use std::io::{Write, Read, Seek, SeekFrom};
use std::fs::OpenOptions;

use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use errors;
use object_value;
use process::RcProcess;

/// Opens a file handle in a particular mode (read-only, write-only, etc).
///
/// This instruction requires X arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The path to the file to open.
/// 3. The register containing a string describing the mode to open the
///    file in.
///
/// The result of this instruction is either a file object or an error
/// object.
///
/// The available file modes supported are the same as those supported by
/// the `fopen()` system call, thus:
///
/// * r: opens a file for reading only
/// * r+: opens a file for reading and writing
/// * w: opens a file for writing only, truncating it if it exists, creating
///   it otherwise
/// * w+: opens a file for reading and writing, truncating it if it exists,
///   creating it if it doesn't exist
/// * a: opens a file for appending, creating it if it doesn't exist
/// * a+: opens a file for reading and appending, creating it if it doesn't
///   exist
pub fn file_open(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let path_ptr = process.get_register(instruction.arg(1)?)?;
    let mode_ptr = process.get_register(instruction.arg(2)?)?;

    let file_proto = machine.state.file_prototype.clone();

    let path = path_ptr.get();
    let mode = mode_ptr.get();

    let path_string = path.value.as_string()?;
    let mode_string = mode.value.as_string()?.as_ref();
    let mut open_opts = OpenOptions::new();

    match mode_string {
        "r" => open_opts.read(true),
        "r+" => open_opts.read(true).write(true).truncate(true).create(true),
        "w" => open_opts.write(true).truncate(true).create(true),
        "w+" => open_opts.read(true).write(true).truncate(true).create(true),
        "a" => open_opts.append(true).create(true),
        "a+" => open_opts.read(true).append(true).create(true),
        _ => set_error!(errors::IO_INVALID_OPEN_MODE, process, register),
    };

    let file = try_io!(open_opts.open(path_string), process, register);

    let obj = process.allocate(object_value::file(file), file_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Writes a string to a file.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the amount of written bytes in.
/// 2. The register containing the file object to write to.
/// 3. The register containing the string to write.
///
/// The result of this instruction is either the amount of written bytes or
/// an error object.
pub fn file_write(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;
    let string_ptr = process.get_register(instruction.arg(2)?)?;

    let mut file = file_ptr.get_mut();
    let string = string_ptr.get();

    let int_proto = machine.state.integer_prototype.clone();
    let mut file = file.value.as_file_mut()?;
    let bytes = string.value.as_string()?.as_bytes();

    let result = try_io!(file.write(bytes), process, register);

    let obj = process.allocate(object_value::integer(result as i64), int_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Reads a number of bytes from a file.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the file to read from.
/// 3. The register containing the amount of bytes to read, if left out
///    all data is read instead.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
pub fn file_read(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;

    let mut file_obj = file_ptr.get_mut();

    let mut buffer = file_reading_buffer!(instruction, process, 2);
    let int_proto = machine.state.integer_prototype.clone();
    let mut file = file_obj.value.as_file_mut()?;

    try_io!(file.read_to_string(&mut buffer), process, register);

    let obj = process.allocate(object_value::string(buffer), int_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Reads an entire line from a file.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the file to read from.
///
/// The result of this instruction is either a string containing the read
/// line, or an error object.
pub fn file_read_line(machine: &Machine,
                      process: &RcProcess,
                      _: &RcCompiledCode,
                      instruction: &Instruction)
                      -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;

    let mut file_obj = file_ptr.get_mut();
    let proto = machine.state.string_prototype.clone();
    let mut file = file_obj.value.as_file_mut()?;
    let mut bytes = Vec::new();

    for result in file.bytes() {
        let byte = try_io!(result, process, register);

        bytes.push(byte);

        if byte == 0xA {
            break;
        }
    }

    let string = try_error!(try_from_utf8!(bytes), process, register);

    let obj = process.allocate(object_value::string(string), proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Flushes a file.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. the register containing the file to flush.
///
/// The resulting object is either boolean true (upon success), or an error
/// object.
pub fn file_flush(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;

    let mut file_obj = file_ptr.get_mut();
    let mut file = file_obj.value.as_file_mut()?;

    try_io!(file.flush(), process, register);

    process.set_register(register, machine.state.true_object.clone());

    Ok(Action::None)
}

/// Returns the size of a file in bytes.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the file.
///
/// The resulting object is either an integer representing the amount of
/// bytes, or an error object.
pub fn file_size(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;

    let file_obj = file_ptr.get();
    let file = file_obj.value.as_file()?;
    let meta = try_io!(file.metadata(), process, register);

    let size = meta.len() as i64;
    let proto = machine.state.integer_prototype.clone();

    let result = process.allocate(object_value::integer(size), proto);

    process.set_register(register, result);

    Ok(Action::None)
}

/// Sets a file cursor to the given offset in bytes.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the input file.
/// 3. The offset to seek to as an integer.
///
/// The resulting object is either an integer representing the new cursor
/// position, or an error object.
pub fn file_seek(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let file_ptr = process.get_register(instruction.arg(1)?)?;
    let offset_ptr = process.get_register(instruction.arg(2)?)?;

    let mut file_obj = file_ptr.get_mut();
    let offset_obj = offset_ptr.get();

    let mut file = file_obj.value.as_file_mut()?;
    let offset = offset_obj.value.as_integer()?;

    ensure_positive_read_size!(instruction, offset);

    let seek_from = SeekFrom::Start(offset as u64);
    let new_offset = try_io!(file.seek(seek_from), process, register);

    let proto = machine.state.integer_prototype.clone();

    let result =
        process.allocate(object_value::integer(new_offset as i64), proto);

    process.set_register(register, result);

    Ok(Action::None)
}
