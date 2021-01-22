//! Functions for working with OS commands.
use crate::external_functions::read_into;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use std::io::Write;
use std::process::{Command, Stdio};

/// Spawns a child process.
///
/// This function requires the following arguments:
///
/// 1. The program name/path to run
/// 2. The arguments to pass to the command
/// 3. The environment variables to pass, as an array of key/value pairs (each
///    key and value are a separate value in the array)
/// 4. What to do with the STDIN stream
/// 5. What to do with the STDOUT stream
/// 6. What to do with the STDERR stream
/// 7. The working directory to use for the command. If the path is empty, no
///    custom directory is set
pub fn child_process_spawn(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let program = arguments[0].string_value()?;
    let args = arguments[1].array_value()?;
    let env = arguments[2].array_value()?;
    let stdin = arguments[3].integer_value()?;
    let stdout = arguments[4].integer_value()?;
    let stderr = arguments[5].integer_value()?;
    let directory = arguments[6].string_value()?;
    let mut cmd = Command::new(program);

    for ptr in args {
        cmd.arg(ptr.string_value()?);
    }

    for pair in env.chunks(2) {
        cmd.env(pair[0].string_value()?, pair[1].string_value()?);
    }

    cmd.stdin(stdio_for(stdin));
    cmd.stdout(stdio_for(stdout));
    cmd.stderr(stdio_for(stderr));

    if !directory.is_empty() {
        cmd.current_dir(directory);
    }

    let child = cmd.spawn()?;

    Ok(process
        .allocate(object_value::command(child), state.child_process_prototype))
}

/// Waits for a command and returns its exit status.
///
/// This method blocks the current thread while waiting.
///
/// This function requires a single argument: the command to wait for.
///
/// This function closes STDIN before waiting.
pub fn child_process_wait(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let status = arguments[0].command_value_mut()?.wait()?;
    let code = status.code().unwrap_or(0) as i64;

    Ok(ObjectPointer::integer(code))
}

/// Waits for a command and returns its exit status, without blocking.
///
/// This method returns immediately if the child has not yet been terminated. If
/// the process hasn't terminated yet, -1 is returned.
///
/// This function requires a single argument: the command to wait for.
pub fn child_process_try_wait(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let status = arguments[0].command_value_mut()?.try_wait()?;
    let code = status.map(|s| s.code().unwrap_or(0)).unwrap_or(-1) as i64;

    Ok(ObjectPointer::integer(code))
}

/// Reads from a child process' STDOUT stream.
///
/// This function requires the following arguments:
///
/// 1. The command to read from
/// 2. The ByteArray to read the data into
/// 3. The number of bytes to read
pub fn child_process_stdout_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let cmd = arguments[0].command_value_mut()?;
    let buff = arguments[1].byte_array_value_mut()?;
    let size = arguments[2].u64_value()?;
    let result = cmd
        .stdout
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(process.allocate_usize(result, state.integer_prototype))
}

/// Reads from a child process' STDERR stream.
///
/// This function requires the following arguments:
///
/// 1. The command to read from
/// 2. The ByteArray to read the data into
/// 3. The number of bytes to read
pub fn child_process_stderr_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let cmd = arguments[0].command_value_mut()?;
    let buff = arguments[1].byte_array_value_mut()?;
    let size = arguments[2].u64_value()?;
    let result = cmd
        .stderr
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(process.allocate_usize(result, state.integer_prototype))
}

/// Writes a ByteArray to a child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to write to.
/// 2. The ByteArray to write.
pub fn child_process_stdin_write_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let cmd = arguments[0].command_value_mut()?;
    let input = arguments[1].byte_array_value()?;
    let size = cmd
        .stdin
        .as_mut()
        .map(|stream| stream.write(&input))
        .unwrap_or(Ok(0))?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Writes a String to a child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to write to.
/// 2. The String to write.
pub fn child_process_stdin_write_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let cmd = arguments[0].command_value_mut()?;
    let input = arguments[1].string_value()?;
    let size = cmd
        .stdin
        .as_mut()
        .map(|stream| stream.write(&input.as_bytes()))
        .unwrap_or(Ok(0))?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Flushes the child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to flush STDIN for.
pub fn child_process_stdin_flush(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let cmd = arguments[0].command_value_mut()?;

    cmd.stdin
        .as_mut()
        .map(|stream| stream.flush())
        .unwrap_or(Ok(()))?;
    Ok(state.nil_object)
}

/// Closes the STDOUT stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stdout_close(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].command_value_mut()?.stdout.take();
    Ok(state.nil_object)
}

/// Closes the STDERR stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stderr_close(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].command_value_mut()?.stderr.take();
    Ok(state.nil_object)
}

/// Closes the STDIN stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stdin_close(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].command_value_mut()?.stdin.take();
    Ok(state.nil_object)
}

fn stdio_for(value: i64) -> Stdio {
    match value {
        1 => Stdio::inherit(),
        2 => Stdio::piped(),
        _ => Stdio::null(),
    }
}

register!(
    child_process_spawn,
    child_process_wait,
    child_process_try_wait,
    child_process_stdout_read,
    child_process_stderr_read,
    child_process_stdout_close,
    child_process_stderr_close,
    child_process_stdin_close,
    child_process_stdin_write_bytes,
    child_process_stdin_write_string,
    child_process_stdin_flush
);
