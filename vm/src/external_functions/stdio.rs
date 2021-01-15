//! Functions for working with the standard input/output streams.
use crate::object_pointer::ObjectPointer;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use std::io::{stderr, stdin, stdout};
use std::io::{Read, Write};

/// Writes a String to STDOUT.
///
/// This function requires a single argument: the input to write.
pub fn stdout_write_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input = arguments[0].string_value()?.as_bytes();
    let size = stdout().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Writes a ByteArray to STDOUT.
///
/// This function requires a single argument: the input to write.
pub fn stdout_write_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input = arguments[0].byte_array_value()?;
    let size = stdout().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Writes a String to STDERR.
///
/// This function requires a single argument: the input to write.
pub fn stderr_write_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input = arguments[0].string_value()?.as_bytes();
    let size = stderr().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Writes a ByteArray to STDERR.
///
/// This function requires a single argument: the input to write.
pub fn stderr_write_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input = arguments[0].byte_array_value()?;
    let size = stderr().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Flushes to STDOUT.
///
/// This function doesn't require any arguments.
pub fn stdout_flush(
    state: &RcState,
    _: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    stdout().flush()?;
    Ok(state.nil_object)
}

/// Flushes to STDERR.
///
/// This function doesn't require any arguments.
pub fn stderr_flush(
    state: &RcState,
    _: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    stderr().flush()?;
    Ok(state.nil_object)
}

/// Reads bytes from STDIN.
///
/// This function requires the following arguments:
///
/// 1. The ByteArray to read the data into.
/// 2. The number of bytes to read.
pub fn stdin_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let buff = arguments[0].byte_array_value_mut()?;
    let amount = arguments[1].u64_value()?;
    let mut stream = stdin();
    let result = if amount > 0 {
        stream.take(amount).read_to_end(buff)?
    } else {
        stream.read_to_end(buff)?
    };

    Ok(process.allocate_usize(result, state.integer_prototype))
}

register!(
    stdout_write_string,
    stdout_write_bytes,
    stderr_write_string,
    stderr_write_bytes,
    stdout_flush,
    stderr_flush,
    stdin_read
);
