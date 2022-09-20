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
use crate::scheduler::process::Thread;
use crate::state::State;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn file_drop(
    _: &State,
    _: &mut Thread,
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
    thread: &mut Thread,
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

    let result = thread.blocking(|| file.seek(seek))? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), result))
}

pub(crate) fn file_flush(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };

    thread.blocking(|| file.flush())?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn file_write_string(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { InkoString::read(&arguments[1]).as_bytes() };
    let written = thread.blocking(|| file.write(input))? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), written))
}

pub(crate) fn file_write_bytes(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { arguments[1].get::<ByteArray>() };
    let written = thread.blocking(|| file.write(input.value()))? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), written))
}

pub(crate) fn file_copy(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let src = unsafe { InkoString::read(&arguments[0]) };
    let dst = unsafe { InkoString::read(&arguments[1]) };
    let copied = thread.blocking(|| fs::copy(src, dst))? as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), copied))
}

pub(crate) fn file_size(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let size = thread.blocking(|| fs::metadata(path))?.len() as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), size))
}

pub(crate) fn file_remove(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    thread.blocking(|| fs::remove_file(path))?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn path_created_at(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path))?;
    let time = system_time_to_timestamp(meta.created()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_modified_at(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path))?;
    let time = system_time_to_timestamp(meta.modified()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_accessed_at(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path))?;
    let time = system_time_to_timestamp(meta.accessed()?);

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn path_is_file(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path));

    if meta.map(|m| m.is_file()).unwrap_or(false) {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn path_is_directory(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path));

    if meta.map(|m| m.is_dir()).unwrap_or(false) {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn path_exists(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let meta = thread.blocking(|| fs::metadata(path));

    if meta.is_ok() {
        Ok(Pointer::true_singleton())
    } else {
        Ok(Pointer::false_singleton())
    }
}

pub(crate) fn file_open_read_only(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true);
    open_file(thread, opts, arguments[0])
}

pub(crate) fn file_open_write_only(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.write(true).truncate(true).create(true);
    open_file(thread, opts, arguments[0])
}

pub(crate) fn file_open_append_only(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.append(true).create(true);
    open_file(thread, opts, arguments[0])
}

pub(crate) fn file_open_read_write(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).write(true).create(true);
    open_file(thread, opts, arguments[0])
}

pub(crate) fn file_open_read_append(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).append(true).create(true);
    open_file(thread, opts, arguments[0])
}

pub(crate) fn file_read(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() };
    let size = unsafe { Int::read(arguments[2]) } as i64;
    let read = thread.blocking(|| read_into(file, buff.value_mut(), size))?;

    Ok(Int::alloc(state.permanent_space.int_class(), read))
}

pub(crate) fn directory_create(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    thread.blocking(|| fs::create_dir(path))?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_create_recursive(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    thread.blocking(|| fs::create_dir_all(path))?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_remove(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    thread.blocking(|| fs::remove_dir(path))?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_remove_recursive(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    thread.blocking(|| fs::remove_dir_all(path))?;
    Ok(Pointer::nil_singleton())
}

pub(crate) fn directory_list(
    state: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let mut paths = Vec::new();

    for entry in thread.blocking(|| fs::read_dir(path))? {
        let entry = entry?;
        let path = entry.path().to_string_lossy().to_string();
        let pointer =
            InkoString::alloc(state.permanent_space.string_class(), path);

        paths.push(pointer);
    }

    Ok(Array::alloc(state.permanent_space.array_class(), paths))
}

fn open_file(
    thread: &mut Thread,
    options: OpenOptions,
    path_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&path_ptr) };

    thread
        .blocking(|| options.open(path))
        .map(Pointer::boxed)
        .map_err(RuntimeError::from)
}

fn system_time_to_timestamp(time: SystemTime) -> f64 {
    let duration = if time < UNIX_EPOCH {
        UNIX_EPOCH.duration_since(time)
    } else {
        time.duration_since(UNIX_EPOCH)
    };

    duration.unwrap().as_secs_f64()
}
