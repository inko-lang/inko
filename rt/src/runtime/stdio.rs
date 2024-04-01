use crate::mem::ByteArray;
use crate::process::ProcessPointer;
use crate::result::Result as InkoResult;
use crate::runtime::helpers::read_into;
use std::io::Write;
use std::io::{stderr, stdin, stdout};

#[no_mangle]
pub unsafe extern "system" fn inko_stdout_write(
    process: ProcessPointer,
    data: *mut u8,
    size: i64,
) -> InkoResult {
    let slice = std::slice::from_raw_parts(data, size as _);

    process
        .blocking(|| stdout().write(slice))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_stderr_write(
    process: ProcessPointer,
    data: *mut u8,
    size: i64,
) -> InkoResult {
    let slice = std::slice::from_raw_parts(data, size as _);

    process
        .blocking(|| stderr().write(slice))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_stdout_flush(process: ProcessPointer) {
    let _ = process.blocking(|| stdout().flush());
}

#[no_mangle]
pub unsafe extern "system" fn inko_stderr_flush(process: ProcessPointer) {
    let _ = process.blocking(|| stderr().flush());
}

#[no_mangle]
pub unsafe extern "system" fn inko_stdin_read(
    process: ProcessPointer,
    buffer: *mut ByteArray,
    size: i64,
) -> InkoResult {
    let buffer = &mut (*buffer).value;

    process
        .blocking(|| read_into(&mut stdin(), buffer, size))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}
