//! VM instruction handlers for working with files.
use std::io::{Write, Read, Seek, SeekFrom};
use std::fs::OpenOptions;

use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use errors;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;

/// File opened for reading, equal to fopen's "r" mode.
const READ: i64 = 0;

/// File opened for writing, equal to fopen's "w" mode.
const WRITE: i64 = 1;

/// File opened for appending, equal to fopen's "a" mode.
const APPEND: i64 = 2;

/// File opened for both reading and writing, equal to fopen's "w+" mode.
const READ_WRITE: i64 = 3;

/// File opened for reading and appending, equal to fopen's "a+" mode.
const READ_APPEND: i64 = 4;

/// The byte indicating the end of a line.
const NEWLINE_BYTE: u8 = 0xA;

/// Opens a file handle in a particular mode (read-only, write-only, etc).
///
/// This instruction requires X arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The path to the file to open.
/// 3. The register containing an integer that specifies the file open mode.
///
/// The result of this instruction is either a file object or an error
/// object.
///
/// The available file modes supported are as follows:
///
/// * 0: read-only
/// * 1: write-only
/// * 2: append-only
/// * 3: read+write
/// * 4: read+append
#[inline(always)]
pub fn file_open(_: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let path_ptr = process.get_register(instruction.arg(1));
    let mode_ptr = process.get_register(instruction.arg(2));

    let path = path_ptr.string_value().unwrap();
    let mode = mode_ptr.integer_value().unwrap();
    let mut open_opts = OpenOptions::new();

    match mode {
        READ => {
            open_opts.read(true);
        }
        WRITE => {
            open_opts.write(true).truncate(true).create(true);
        }
        APPEND => {
            open_opts.read(true).write(true).truncate(true).create(true);
        }
        READ_WRITE => {
            open_opts.append(true).create(true);
        }
        READ_APPEND => {
            open_opts.read(true).append(true).create(true);
        }
        _ => {}
    };

    let object = match open_opts.open(path) {
        Ok(file) => process.allocate_without_prototype(object_value::file(file)),
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, object);
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
#[inline(always)]
pub fn file_write(_: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));
    let string_ptr = process.get_register(instruction.arg(2));

    let mut file = file_ptr.file_value_mut().unwrap();
    let bytes = string_ptr.string_value().unwrap().as_bytes();

    let obj = match file.write(bytes) {
        Ok(num_bytes) => ObjectPointer::integer(num_bytes as i64),
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}

/// Reads the all data from a file.
///
/// This instruction takes 2 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the file to read from.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
#[inline(always)]
pub fn file_read(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));

    let mut file = file_ptr.file_value_mut().unwrap();
    let mut buffer = String::new();

    let obj = match file.read_to_string(&mut buffer) {
        Ok(_) => {
            process.allocate(object_value::string(buffer),
                             machine.state.string_prototype)
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}

/// Reads a given number of bytes from a file.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the file to read from.
/// 3. The register containing the number of bytes to read, as a positive
///    integer.
///
/// The result of this instruction is either a string containing the data
/// read, or an error object.
#[inline(always)]
pub fn file_read_exact(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));
    let size_ptr = process.get_register(instruction.arg(2));

    let mut file = file_ptr.file_value_mut().unwrap();
    let size = size_ptr.integer_value().unwrap() as usize;
    let mut buffer = String::with_capacity(size);

    let obj = match file.take(size as u64).read_to_string(&mut buffer) {
        Ok(_) => {
            process.allocate(object_value::string(buffer),
                             machine.state.string_prototype)
        }
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
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
#[inline(always)]
pub fn file_read_line(machine: &Machine,
                      process: &RcProcess,
                      _: &RcCompiledCode,
                      instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));

    let mut file = file_ptr.file_value_mut().unwrap();
    let mut buffer = Vec::new();

    for result in file.bytes() {
        match result {
            Ok(byte) => {
                buffer.push(byte);

                if byte == NEWLINE_BYTE {
                    break;
                }
            }
            Err(error) => {
                process.set_register(register, io_error_code!(process, error));

                return;
            }
        }
    }

    let obj = match String::from_utf8(buffer) {
        Ok(string) => {
            process.allocate(object_value::string(string),
                             machine.state.string_prototype)
        }
        Err(_) => {
            let code = errors::string::invalid_utf8();

            process.allocate_without_prototype(object_value::error(code))
        }
    };

    process.set_register(register, obj);
}

/// Flushes a file.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. the register containing the file to flush.
///
/// The resulting object is either the file itself upon success, or an error
/// object.
#[inline(always)]
pub fn file_flush(_: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));

    let mut file = file_ptr.file_value_mut().unwrap();

    let obj = match file.flush() {
        Ok(_) => file_ptr,
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
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
#[inline(always)]
pub fn file_size(_: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));
    let file = file_ptr.file_value().unwrap();

    let obj = match file.metadata() {
        Ok(meta) => ObjectPointer::integer(meta.len() as i64),
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}

/// Sets a file cursor to the given offset in bytes.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The register containing the input file.
/// 3. The offset to seek to as an integer. This integer must be greater than 0.
///
/// The resulting object is either an integer representing the new cursor
/// position, or an error object.
#[inline(always)]
pub fn file_seek(_: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let file_ptr = process.get_register(instruction.arg(1));
    let offset_ptr = process.get_register(instruction.arg(2));

    let mut file = file_ptr.file_value_mut().unwrap();
    let offset = offset_ptr.integer_value().unwrap();

    let obj = match file.seek(SeekFrom::Start(offset as u64)) {
        Ok(new_offset) => ObjectPointer::integer(new_offset as i64),
        Err(error) => io_error_code!(process, error),
    };

    process.set_register(register, obj);
}
