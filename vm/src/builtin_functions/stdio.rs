//! Functions for working with the standard input/output streams.
use crate::builtin_functions::read_into;
use crate::mem::{ByteArray, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;
use std::io::Write;
use std::io::{stderr, stdin, stdout};

pub(crate) fn stdout_write_string(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { InkoString::read(&arguments[0]).as_bytes() };
    let size = stdout().write(input)? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), size))
}

pub(crate) fn stdout_write_bytes(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { arguments[0].get::<ByteArray>() }.value();
    let size = stdout().write(input)? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), size))
}

pub(crate) fn stderr_write_string(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { InkoString::read(&arguments[0]).as_bytes() };
    let size = stderr().write(input)? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), size))
}

pub(crate) fn stderr_write_bytes(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { arguments[0].get::<ByteArray>() }.value();
    let size = stderr().write(input)? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), size))
}

pub(crate) fn stdout_flush(
    _: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    stdout().flush()?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn stderr_flush(
    _: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    stderr().flush()?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn stdin_read(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let buff = unsafe { arguments[0].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { Int::read(arguments[1]) };
    let result = read_into(&mut stdin(), buff, size)?;

    Ok(Int::alloc(state.permanent_space.int_class(), result))
}
