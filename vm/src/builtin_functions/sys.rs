//! Functions for working with the underlying system.
use crate::builtin_functions::read_into;
use crate::mem::{Array, ByteArray, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;
use num_cpus;
use std::io::Write;
use std::process::{Child, Command, Stdio};

pub(crate) fn child_process_spawn(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let program = unsafe { InkoString::read(&arguments[0]) };
    let args = unsafe { arguments[1].get::<Array>() }.value();
    let env = unsafe { arguments[2].get::<Array>() }.value();
    let stdin = unsafe { Int::read(arguments[3]) };
    let stdout = unsafe { Int::read(arguments[4]) };
    let stderr = unsafe { Int::read(arguments[5]) };
    let directory = unsafe { InkoString::read(&arguments[6]) };
    let mut cmd = Command::new(program);

    for ptr in args {
        cmd.arg(unsafe { InkoString::read(&*ptr) });
    }

    for pair in env.chunks(2) {
        unsafe {
            cmd.env(InkoString::read(&pair[0]), InkoString::read(&pair[1]));
        }
    }

    cmd.stdin(stdio_for(stdin));
    cmd.stdout(stdio_for(stdout));
    cmd.stderr(stdio_for(stderr));

    if !directory.is_empty() {
        cmd.current_dir(directory);
    }

    Ok(Pointer::boxed(cmd.spawn()?))
}

/// Waits for a command and returns its exit status.
///
/// This method blocks the current thread while waiting.
///
/// This function closes STDIN before waiting.
pub(crate) fn child_process_wait(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let status = child.wait()?;
    let code = status.code().unwrap_or(0) as i64;

    Ok(Pointer::int(code))
}

/// Waits for a command and returns its exit status, without blocking.
///
/// This method returns immediately if the child has not yet been terminated. If
/// the process hasn't terminated yet, -1 is returned.
///
/// This function requires a single argument: the command to wait for.
pub(crate) fn child_process_try_wait(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let status = child.try_wait()?;
    let code = status.map(|s| s.code().unwrap_or(0)).unwrap_or(-1) as i64;

    Ok(Pointer::int(code))
}

pub(crate) fn child_process_stdout_read(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { Int::read(arguments[2]) };
    let value = child
        .stdout
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

pub(crate) fn child_process_stderr_read(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { Int::read(arguments[2]) };
    let value = child
        .stderr
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

pub(crate) fn child_process_stdin_write_bytes(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let input = unsafe { arguments[1].get::<ByteArray>() }.value();
    let value = child
        .stdin
        .as_mut()
        .map(|stream| stream.write(input))
        .unwrap_or(Ok(0))?;

    Ok(Int::alloc(state.permanent_space.int_class(), value as i64))
}

pub(crate) fn child_process_stdin_write_string(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let input = unsafe { InkoString::read(&arguments[1]) };
    let value = child
        .stdin
        .as_mut()
        .map(|stream| stream.write(input.as_bytes()))
        .unwrap_or(Ok(0))?;

    Ok(Int::alloc(state.permanent_space.int_class(), value as i64))
}

pub(crate) fn child_process_stdin_flush(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stdin.as_mut().map(|stream| stream.flush()).unwrap_or(Ok(()))?;

    Ok(Pointer::nil_singleton())
}

pub(crate) fn child_process_stdout_close(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stdout.take();
    Ok(Pointer::nil_singleton())
}

pub(crate) fn child_process_stderr_close(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stderr.take();
    Ok(Pointer::nil_singleton())
}

pub(crate) fn child_process_stdin_close(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stdin.take();
    Ok(Pointer::nil_singleton())
}

pub(crate) fn child_process_drop(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Child>();
    }

    Ok(Pointer::nil_singleton())
}

pub(crate) fn cpu_cores(
    _: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Pointer::int(num_cpus::get() as i64))
}

fn stdio_for(value: i64) -> Stdio {
    match value {
        1 => Stdio::inherit(),
        2 => Stdio::piped(),
        _ => Stdio::null(),
    }
}
