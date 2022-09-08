//! Functions for working with the file system.
//!
//! Files aren't allocated onto the Inko heap. Instead, we allocate them using
//! Rust's allocator and convert them into an Inko pointer. Dropping a file
//! involves turning that pointer back into a File, then dropping the Rust
//! object.
//!
//! This approach means the VM doesn't need to know anything about what objects
//! to use for certain files, how to store file paths, etc; instead we can keep
//! all that in the standard library.
use crate::builtin_functions::read_into;
use crate::mem::{Array, ByteArray, Float, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn file_drop(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<File>();
    }

    Ok(Pointer::nil_singleton())
}

pub(crate) fn file_seek(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let offset = unsafe { Int::read(arguments[1]) };
    let seek = if offset < 0 {
        SeekFrom::End(offset)
    } else {
        SeekFrom::Start(offset as u64)
    };

    let result = file.seek(seek)? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), result))
}

pub(crate) fn file_flush(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };

    file.flush()?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn file_write_string(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { InkoString::read(&arguments[1]).as_bytes() };

    Ok(Int::alloc(state.permanent_space.int_class(), file.write(input)? as i64))
}

pub(crate) fn file_write_bytes(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { arguments[1].get::<ByteArray>() };

    Ok(Int::alloc(
        state.permanent_space.int_class(),
        file.write(input.value())? as i64,
    ))
}

pub(crate) fn file_copy(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let src = unsafe { InkoString::read(&arguments[0]) };
    let dst = unsafe { InkoString::read(&arguments[1]) };

    Ok(Int::alloc(
        state.permanent_space.int_class(),
        fs::copy(src, dst)? as i64,
    ))
}

pub(crate) fn file_size(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    Ok(Int::alloc(
        state.permanent_space.int_class(),
        fs::metadata(path)?.len() as i64,
    ))
}

pub(crate) fn file_remove(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_file(path)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn path_created_at(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time = system_time_to_timestamp(fs::metadata(path)?.created()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_modified_at(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time = system_time_to_timestamp(fs::metadata(path)?.modified()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_accessed_at(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time = system_time_to_timestamp(fs::metadata(path)?.accessed()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_is_file(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn path_is_directory(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn path_exists(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).is_ok() {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn file_open_read_only(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true);
    open_file(opts, arguments[0])
}

pub(crate) fn file_open_write_only(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.write(true).truncate(true).create(true);
    open_file(opts, arguments[0])
}

pub(crate) fn file_open_append_only(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.append(true).create(true);
    open_file(opts, arguments[0])
}

pub(crate) fn file_open_read_write(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).write(true).create(true);
    open_file(opts, arguments[0])
}

pub(crate) fn file_open_read_append(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).append(true).create(true);
    open_file(opts, arguments[0])
}

pub(crate) fn file_read(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() };
    let size = unsafe { Int::read(arguments[2]) } as i64;

    Ok(Int::alloc(
        state.permanent_space.int_class(),
        read_into(file, buff.value_mut(), size)?,
    ))
}

pub(crate) fn directory_create(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::create_dir(path)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_create_recursive(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::create_dir_all(path)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_remove(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_dir(path)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_remove_recursive(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_dir_all(path)?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_list(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let mut paths = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path().to_string_lossy().to_string();
        let pointer =
            InkoString::alloc(state.permanent_space.string_class(), path);

        paths.push(pointer);
    }

    Ok(Array::alloc(state.permanent_space.array_class(), paths))
}

fn open_file(
    options: OpenOptions,
    path_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&path_ptr) };

    options
        .open(path)
        .map(|fd| Pointer::boxed(fd))
        .map_err(|e| RuntimeError::from(e))
}

fn system_time_to_timestamp(time: SystemTime) -> f64 {
    let duration = if time < UNIX_EPOCH {
        UNIX_EPOCH.duration_since(time)
    } else {
        time.duration_since(UNIX_EPOCH)
    };

    duration.unwrap().as_secs_f64()
}
