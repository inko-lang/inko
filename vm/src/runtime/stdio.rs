use crate::mem::{ByteArray, Int, Nil, String as InkoString};
use crate::process::ProcessPointer;
use crate::result::Result as InkoResult;
use crate::runtime::helpers::read_into;
use crate::state::State;
use std::io::Write;
use std::io::{stderr, stdin, stdout};

#[no_mangle]
pub unsafe extern "system" fn inko_stdout_write_string(
    state: *const State,
    process: ProcessPointer,
    input: *const InkoString,
) -> InkoResult {
    let input = InkoString::read(input).as_bytes();

    process
        .blocking(|| stdout().write(input))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_stdout_write_bytes(
    state: *const State,
    process: ProcessPointer,
    input: *mut ByteArray,
) -> InkoResult {
    let input = &(*input).value;

    process
        .blocking(|| stdout().write(input))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_stderr_write_string(
    state: *const State,
    process: ProcessPointer,
    input: *const InkoString,
) -> InkoResult {
    let input = InkoString::read(input).as_bytes();

    process
        .blocking(|| stderr().write(input))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_stderr_write_bytes(
    state: *const State,
    process: ProcessPointer,
    input: *mut ByteArray,
) -> InkoResult {
    let input = &(*input).value;

    process
        .blocking(|| stderr().write(input))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_stdout_flush(
    state: *const State,
    process: ProcessPointer,
) -> *const Nil {
    let _ = process.blocking(|| stdout().flush());

    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_stderr_flush(
    state: *const State,
    process: ProcessPointer,
) -> *const Nil {
    let _ = process.blocking(|| stderr().flush());

    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_stdin_read(
    state: *const State,
    process: ProcessPointer,
    buffer: *mut ByteArray,
    size: i64,
) -> InkoResult {
    let buffer = &mut (*buffer).value;

    process
        .blocking(|| read_into(&mut stdin(), buffer, size))
        .map(|size| InkoResult::Ok(Int::new((*state).int_class, size) as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}
