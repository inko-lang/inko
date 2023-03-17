use crate::mem::{
    Array, Bool, ByteArray, Float, Int, Nil, String as InkoString,
};
use crate::process::ProcessPointer;
use crate::result::Result as InkoResult;
use crate::runtime::helpers::read_into;
use crate::state::State;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::time::{SystemTime, UNIX_EPOCH};

#[no_mangle]
pub unsafe extern "system" fn inko_file_drop(
    state: *const State,
    file: *mut File,
) -> *const Nil {
    drop(Box::from_raw(file));
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_seek(
    state: *const State,
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
        .map(
            |res| InkoResult::Ok(Int::new((*state).int_class, res as i64) as _),
        )
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_flush(
    state: *const State,
    process: ProcessPointer,
    file: *mut File,
) -> InkoResult {
    process
        .blocking(|| (*file).flush())
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_write_string(
    state: *const State,
    process: ProcessPointer,
    file: *mut File,
    input: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| (*file).write(InkoString::read(input).as_bytes()))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_write_bytes(
    state: *const State,
    process: ProcessPointer,
    file: *mut File,
    input: *mut ByteArray,
) -> InkoResult {
    process
        .blocking(|| (*file).write(&(*input).value))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_copy(
    state: *const State,
    process: ProcessPointer,
    from: *const InkoString,
    to: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::copy(InkoString::read(from), InkoString::read(to)))
        .map(|size| {
            InkoResult::Ok(Int::new((*state).int_class, size as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_size(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .map(|meta| {
            InkoResult::Ok(Int::new((*state).int_class, meta.len() as i64) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_remove(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_file(InkoString::read(path)))
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_created_at(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.created())
        .map(|time| system_time_to_timestamp(time))
        .map(|time| {
            InkoResult::Ok(Float::alloc((*state).float_class, time) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_modified_at(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.modified())
        .map(|time| system_time_to_timestamp(time))
        .map(|time| {
            InkoResult::Ok(Float::alloc((*state).float_class, time) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_accessed_at(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::metadata(InkoString::read(path)))
        .and_then(|meta| meta.accessed())
        .map(|time| system_time_to_timestamp(time))
        .map(|time| {
            InkoResult::Ok(Float::alloc((*state).float_class, time) as _)
        })
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_is_file(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> *const Bool {
    let state = &*state;
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.map(|m| m.is_file()).unwrap_or(false) {
        state.true_singleton
    } else {
        state.false_singleton
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_is_directory(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> *const Bool {
    let state = &*state;
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.map(|m| m.is_dir()).unwrap_or(false) {
        state.true_singleton
    } else {
        state.false_singleton
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_path_exists(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> *const Bool {
    let state = &*state;
    let meta = process.blocking(|| fs::metadata(InkoString::read(path)));

    if meta.is_ok() {
        state.true_singleton
    } else {
        state.false_singleton
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

    open_file(process, opts, path)
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_file_read(
    state: *const State,
    process: ProcessPointer,
    file: *mut File,
    buffer: *mut ByteArray,
    size: i64,
) -> InkoResult {
    let file = &mut *file;
    let buffer = &mut (*buffer).value;

    process
        .blocking(|| read_into(file, buffer, size))
        .map(|size| InkoResult::Ok(Int::new((*state).int_class, size) as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_create(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::create_dir(InkoString::read(path)))
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_create_recursive(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::create_dir_all(InkoString::read(path)))
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_remove(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_dir(InkoString::read(path)))
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_remove_all(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    process
        .blocking(|| fs::remove_dir_all(InkoString::read(path)))
        .map(|_| InkoResult::Ok((*state).nil_singleton as _))
        .unwrap_or_else(|err| InkoResult::io_error(err))
}

#[no_mangle]
pub unsafe extern "system" fn inko_directory_list(
    state: *const State,
    process: ProcessPointer,
    path: *const InkoString,
) -> InkoResult {
    let state = &*state;
    let mut paths = Vec::new();
    let entries =
        match process.blocking(|| fs::read_dir(InkoString::read(path))) {
            Ok(entries) => entries,
            Err(err) => return InkoResult::io_error(err),
        };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => return InkoResult::io_error(err),
        };

        let path = entry.path().to_string_lossy().to_string();
        let pointer = InkoString::alloc(state.string_class, path);

        paths.push(pointer as *mut u8);
    }

    InkoResult::Ok(Array::alloc(state.array_class, paths) as _)
}

unsafe fn open_file(
    process: ProcessPointer,
    options: OpenOptions,
    path: *const InkoString,
) -> Result<InkoResult, io::Error> {
    process
        .blocking(|| options.open(InkoString::read(path)))
        .map(|file| InkoResult::Ok(Box::into_raw(Box::new(file)) as _))
}

fn system_time_to_timestamp(time: SystemTime) -> f64 {
    let duration = if time < UNIX_EPOCH {
        UNIX_EPOCH.duration_since(time)
    } else {
        time.duration_since(UNIX_EPOCH)
    };

    duration.unwrap().as_secs_f64()
}
