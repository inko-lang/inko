//! Functions for working with the file system.
use crate::date_time::DateTime;
use crate::external_functions::read_into;
use crate::file::File;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use num_traits::Signed;
use num_traits::ToPrimitive;
use std::fs;
use std::io::{Seek, SeekFrom, Write};

/// Returns the path of a file.
///
/// This function requires one argument: the file to get the path of.
pub fn file_path(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(*arguments[0].file_value()?.path())
}

/// Seeks a file to an offset.
///
/// This function takes the following arguments:
///
/// 1. The file to seek for.
/// 2. The byte offset to seek to.
pub fn file_seek(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file_ptr = arguments[0];
    let offset_ptr = arguments[1];
    let file = file_ptr.file_value_mut()?;
    let seek = if offset_ptr.is_bigint() {
        let big_offset = offset_ptr.bigint_value()?;

        if big_offset.is_negative() {
            SeekFrom::End(big_offset.to_i64().unwrap_or(i64::MIN))
        } else {
            SeekFrom::Start(big_offset.to_u64().unwrap_or(u64::MAX))
        }
    } else {
        let offset = offset_ptr.integer_value()?;

        if offset < 0 {
            SeekFrom::End(offset)
        } else {
            SeekFrom::Start(offset as u64)
        }
    };

    let cursor = file.get_mut().seek(seek)?;

    Ok(process.allocate_u64(cursor, state.integer_prototype))
}

/// Flushes a file.
///
/// This function requires a single argument: the file to flush.
pub fn file_flush(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].file_value_mut()?.get_mut().flush()?;
    Ok(state.nil_object)
}

/// Writes a String to a file.
///
/// This function requires the following arguments:
///
/// 1. The file to write to.
/// 2. The input to write.
pub fn file_write_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = arguments[0].file_value_mut()?;
    let input = arguments[1].string_value()?.as_bytes();
    let size = file.get_mut().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Writes a ByteArray to a file.
///
/// This function requires the following arguments:
///
/// 1. The file to write to.
/// 2. The input to write.
pub fn file_write_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = arguments[0].file_value_mut()?;
    let input = arguments[1].byte_array_value()?;
    let size = file.get_mut().write(&input)?;

    Ok(process.allocate_usize(size, state.integer_prototype))
}

/// Copies a file from one location to another.
///
/// This function requires the following arguments:
///
/// 1. The path to the file to copy.
/// 2. The path to copy the file to.
pub fn file_copy(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let src = arguments[0].string_value()?;
    let dst = arguments[1].string_value()?;
    let bytes_copied = fs::copy(src, dst)?;

    Ok(process.allocate_u64(bytes_copied, state.integer_prototype))
}

/// Returns the size of a file in bytes.
///
/// This function requires a single argument: the path of the file to return the
/// size for.
pub fn file_size(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;
    let meta = fs::metadata(path)?;

    Ok(process.allocate_u64(meta.len(), state.integer_prototype))
}

/// Removes a file.
///
/// This function requires a single argument: the path to the file to remove.
pub fn file_remove(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    fs::remove_file(arguments[0].string_value()?)?;
    Ok(state.nil_object)
}

/// Returns the creation time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_created_at(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;
    let time = fs::metadata(&path)?.created()?;

    Ok(allocate_time(
        state,
        process,
        DateTime::from_system_time(time),
    ))
}

/// Returns the modification time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_modified_at(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;
    let time = fs::metadata(&path)?.modified()?;

    Ok(allocate_time(
        state,
        process,
        DateTime::from_system_time(time),
    ))
}

/// Returns the access time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_accessed_at(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;
    let time = fs::metadata(&path)?.accessed()?;

    Ok(allocate_time(
        state,
        process,
        DateTime::from_system_time(time),
    ))
}

/// Checks if a path is a file.
///
/// This function requires a single argument: the path to check.
pub fn path_is_file(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Checks if a path is a directory.
///
/// This function requires a single argument: the path to check.
pub fn path_is_directory(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Checks if a path exists.
///
/// This function requires a single argument: the path to check.
pub fn path_exists(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    if fs::metadata(path).is_ok() {
        Ok(state.true_object)
    } else {
        Ok(state.false_object)
    }
}

/// Opens a file in read-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_only(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = File::read_only(arguments[0])?;
    let proto = state.read_only_file_prototype;

    Ok(process.allocate(object_value::file(file), proto))
}

/// Opens a file in write-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_write_only(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = File::write_only(arguments[0])?;
    let proto = state.write_only_file_prototype;

    Ok(process.allocate(object_value::file(file), proto))
}

/// Opens a file in append-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_append_only(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = File::append_only(arguments[0])?;
    let proto = state.write_only_file_prototype;

    Ok(process.allocate(object_value::file(file), proto))
}

/// Opens a file for both reading and writing.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_write(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = File::read_write(arguments[0])?;
    let proto = state.read_write_file_prototype;

    Ok(process.allocate(object_value::file(file), proto))
}

/// Opens a file for both reading and appending.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_append(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = File::read_append(arguments[0])?;
    let proto = state.read_write_file_prototype;

    Ok(process.allocate(object_value::file(file), proto))
}

/// Reads bytes from a file into a ByteArray.
///
/// This function requires the following arguments:
///
/// 1. The file to read from.
/// 2. A ByteArray to read into.
/// 3. The number of bytes to read.
pub fn file_read(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let file = arguments[0].file_value_mut()?;
    let buff = arguments[1].byte_array_value_mut()?;
    let size = arguments[2].u64_value().ok();
    let stream = file.get_mut();
    let result = read_into(stream, buff, size)?;

    Ok(process.allocate_usize(result, state.integer_prototype))
}

fn allocate_time(
    state: &RcState,
    process: &RcProcess,
    time: DateTime,
) -> ObjectPointer {
    let offset = ObjectPointer::integer(time.utc_offset());
    let seconds = process
        .allocate(object_value::float(time.timestamp()), state.float_prototype);

    process.allocate(
        object_value::array(vec![seconds, offset]),
        state.array_prototype,
    )
}

/// Creates a new directory.
///
/// This function requires one argument: the path of the directory to create.
pub fn directory_create(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    fs::create_dir(path)?;
    Ok(state.nil_object)
}

/// Creates a new directory and any missing parent directories.
///
/// This function requires one argument: the path of the directory to create.
pub fn directory_create_recursive(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    fs::create_dir_all(path)?;
    Ok(state.nil_object)
}

/// Removes a directory.
///
/// This function requires one argument: the path of the directory to remove.
pub fn directory_remove(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    fs::remove_dir(path)?;
    Ok(state.nil_object)
}

/// Removes a directory and all its contents.
///
/// This function requires one argument: the path of the directory to remove.
pub fn directory_remove_recursive(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;

    fs::remove_dir_all(path)?;
    Ok(state.nil_object)
}

/// Returns the contents of a directory.
///
/// This function requires one argument: the path of the directory to list.
pub fn directory_list(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = arguments[0].string_value()?;
    let mut paths = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path().to_string_lossy().to_string();
        let pointer = process
            .allocate(object_value::string(path), state.string_prototype);

        paths.push(pointer);
    }

    let paths_ptr =
        process.allocate(object_value::array(paths), state.array_prototype);

    Ok(paths_ptr)
}

register!(
    file_path,
    file_seek,
    file_flush,
    file_write_string,
    file_write_bytes,
    file_copy,
    file_size,
    file_remove,
    path_created_at,
    path_modified_at,
    path_accessed_at,
    path_is_file,
    path_is_directory,
    path_exists,
    file_open_read_only,
    file_open_write_only,
    file_open_append_only,
    file_open_read_write,
    file_open_read_append,
    file_read,
    directory_create,
    directory_create_recursive,
    directory_remove,
    directory_remove_recursive,
    directory_list
);
