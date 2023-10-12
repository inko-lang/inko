use crate::mem::{ByteArray, String as InkoString};
use crate::process::ProcessPointer;
use crate::result::Result as InkoResult;
use crate::runtime::helpers::read_into;
use crate::state::State;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[no_mangle]
pub unsafe extern "system" fn inko_file_drop(file: *mut File) {
    drop(Box::from_raw(file));
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_seek(
    process: ProcessPointer,
    file: *mut File,
    offset: i64,
) -> InkoResult {
    let seek = if offset < 0 {
        SeekFrom::End(offset)
    } else {
        SeekFrom::Start(offset as u64)
    };

    process
        .blocking(|| (*file).seek(seek))
        .map(|res| InkoResult::ok(res as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_flush(
    process: ProcessPointer,
    file: *mut File,
) -> InkoResult {
    process
        .blocking(|| (*file).flush())
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_write_string(
    process: ProcessPointer,
    file: *mut File,
    input: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| (*file).write(InkoString::read(input).as_bytes()))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_write_bytes(
    process: ProcessPointer,
    file: *mut File,
    input: *mut ByteArray,
) -> InkoResult {
    process
        .blocking(|| (*file).write(&(*input).value))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_copy(
    process: ProcessPointer,
    from: *const InkoString,
    to: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::copy(InkoString::read(from), InkoString::read(to)))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_size(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .map(|meta| InkoResult::ok(meta.len() as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_remove(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_file(InkoString::read(path)))
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_created_at(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.created())
        .map(system_time_to_timestamp)
        .map(|time| InkoResult::ok(time.to_bits() as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_modified_at(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.modified())
        .map(system_time_to_timestamp)
        .map(|time| InkoResult::ok(time.to_bits() as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_accessed_at(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.accessed())
        .map(system_time_to_timestamp)
        .map(|time| InkoResult::ok(time.to_bits() as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_expand(
    state: *const State,
    path: *const InkoString,
) -> InkoResult {
    let path = InkoString::read(path);

    PathBuf::from(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map(|p| {
            InkoResult::ok(InkoString::alloc((*state).string_class, p) as _)
        })
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_is_file(
    process: ProcessPointer,
    path: *const InkoString,
) -> i64 {
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.map(|m| m.is_file()).unwrap_or(false) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_is_directory(
    process: ProcessPointer,
    path: *const InkoString,
) -> i64 {
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.map(|m| m.is_dir()).unwrap_or(false) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_exists(
    process: ProcessPointer,
    path: *const InkoString,
) -> i64 {
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.is_ok() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_open(
    process: ProcessPointer,
    path: *const InkoString,
    mode: i64,
) -> InkoResult {
    let mut opts = OpenOptions::new();

    match mode {
        0 => opts.read(true), // Read-only
        1 => opts.write(true).truncate(true).create(true), // Write-only
        2 => opts.append(true).create(true), // Append-only
        3 => opts.read(true).write(true).create(true), // Read-write
        _ => opts.read(true).append(true).create(true), // Read-append
    };

    open_file(process, opts, path).unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_read(
    process: ProcessPointer,
    file: *mut File,
    buffer: *mut ByteArray,
    size: i64,
) -> InkoResult {
    let file = &mut *file;
    let buffer = &mut (*buffer).value;

    process
        .blocking(|| read_into(file, buffer, size))
        .map(|size| InkoResult::ok(size as _))
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_create(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::create_dir(InkoString::read(path)))
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_create_recursive(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::create_dir_all(InkoString::read(path)))
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_remove(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_dir(InkoString::read(path)))
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_remove_recursive(
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_dir_all(InkoString::read(path)))
        .map(|_| InkoResult::none())
        .unwrap_or_else(InkoResult::io_error)
}

unsafe fn open_file(
    process: ProcessPointer,
    options: OpenOptions,
    path: *const InkoString,
) -> Result<InkoResult, io::Error> {
    process
        .blocking(|| options.open(InkoString::read(path)))
        .map(|file| InkoResult::ok(Box::into_raw(Box::new(file)) as _))
}

fn system_time_to_timestamp(time: SystemTime) -> f64 {
    let duration = if time < UNIX_EPOCH {
        UNIX_EPOCH.duration_since(time)
    } else {
        time.duration_since(UNIX_EPOCH)
    };

    duration.unwrap().as_secs_f64()
}
