//! VM functions for working with IO.
use crate::filesystem;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use num_traits::ToPrimitive;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};

/// File opened for reading, equal to fopen's "r" mode.
const READ: i64 = 0;

/// File opened for writing, equal to fopen's "w" mode.
const WRITE: i64 = 1;

/// File opened for appending, equal to fopen's "a" mode.
const APPEND: i64 = 2;

/// File opened for both reading and writing, equal to fopen's "w+" mode.
const READ_WRITE: i64 = 3;

/// File opened for reading and appending, equal to fopen's "a+" mode.
const READ_APPEND: i64 = 4;

#[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
pub fn buffer_to_write(buffer: &ObjectPointer) -> Result<&[u8], RuntimeError> {
    let buff = if buffer.is_string() {
        buffer.string_value()?.as_bytes()
    } else {
        buffer.byte_array_value()?
    };

    Ok(buff)
}

pub fn io_write<W: Write>(
    state: &RcState,
    process: &RcProcess,
    output: &mut W,
    to_write: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let written = output.write(buffer_to_write(&to_write)?)?;

    Ok(process.allocate_usize(written, state.integer_prototype))
}

#[inline(always)]
pub fn stdout_write(
    state: &RcState,
    process: &RcProcess,
    to_write: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let mut output = io::stdout();

    io_write(state, process, &mut output, to_write)
}

#[inline(always)]
pub fn stdout_flush() -> Result<(), RuntimeError> {
    io::stdout().flush()?;
    Ok(())
}

#[inline(always)]
pub fn stderr_write(
    state: &RcState,
    process: &RcProcess,
    to_write: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let mut output = io::stderr();

    io_write(state, process, &mut output, to_write)
}

#[inline(always)]
pub fn stderr_flush() -> Result<(), RuntimeError> {
    io::stdout().flush()?;
    Ok(())
}

#[inline(always)]
pub fn stdin_read(
    state: &RcState,
    process: &RcProcess,
    buffer_ptr: ObjectPointer,
    amount: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let mut input = io::stdin();
    let buffer = buffer_ptr.byte_array_value_mut()?;

    io_read(state, process, &mut input, buffer, amount)
}

#[inline(always)]
pub fn file_write(
    state: &RcState,
    process: &RcProcess,
    file_ptr: ObjectPointer,
    to_write: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let file = file_ptr.file_value_mut()?;

    io_write(state, process, file, to_write)
}

#[inline(always)]
pub fn file_flush(file_ptr: ObjectPointer) -> Result<(), RuntimeError> {
    let file = file_ptr.file_value_mut()?;

    file.flush()?;
    Ok(())
}

#[inline(always)]
pub fn file_read(
    state: &RcState,
    process: &RcProcess,
    file_ptr: ObjectPointer,
    buffer_ptr: ObjectPointer,
    amount: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let mut input = file_ptr.file_value_mut()?;
    let buffer = buffer_ptr.byte_array_value_mut()?;

    io_read(state, process, &mut input, buffer, amount)
}

#[inline(always)]
pub fn file_open(
    process: &RcProcess,
    proto_ptr: ObjectPointer,
    path_ptr: ObjectPointer,
    mode_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;
    let mode = mode_ptr.integer_value()?;
    let open_opts = options_for_integer(mode)?;
    let file = open_opts.open(path)?;

    Ok(process.allocate(object_value::file(file), proto_ptr))
}

#[inline(always)]
pub fn file_size(
    state: &RcState,
    process: &RcProcess,
    path_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;
    let meta = fs::metadata(path)?;

    Ok(process.allocate_u64(meta.len(), state.integer_prototype))
}

#[inline(always)]
pub fn file_seek(
    state: &RcState,
    process: &RcProcess,
    file_ptr: ObjectPointer,
    offset_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let file = file_ptr.file_value_mut()?;

    let offset = if offset_ptr.is_bigint() {
        let big_offset = offset_ptr.bigint_value()?;

        if let Some(offset) = big_offset.to_u64() {
            offset
        } else {
            return Err(RuntimeError::Panic(format!(
                "{} is too big for a seek offset",
                big_offset
            )));
        }
    } else {
        let offset = offset_ptr.integer_value()?;

        if offset < 0 {
            return Err(RuntimeError::Panic(format!(
                "{} is not a valid seek offset",
                offset
            )));
        }

        offset as u64
    };

    let cursor = file.seek(SeekFrom::Start(offset))?;

    Ok(process.allocate_u64(cursor, state.integer_prototype))
}

#[inline(always)]
pub fn file_remove(
    state: &RcState,
    path_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path_str = path_ptr.string_value()?;

    fs::remove_file(path_str)?;

    Ok(state.nil_object)
}

#[inline(always)]
pub fn file_copy(
    state: &RcState,
    process: &RcProcess,
    src_ptr: ObjectPointer,
    dst_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let src = src_ptr.string_value()?;
    let dst = dst_ptr.string_value()?;
    let bytes_copied = fs::copy(src, dst)?;

    Ok(process.allocate_u64(bytes_copied, state.integer_prototype))
}

#[inline(always)]
pub fn file_type(
    path_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;
    let file_type = filesystem::type_of_path(path);

    Ok(ObjectPointer::integer(file_type))
}

#[inline(always)]
pub fn file_time(
    state: &RcState,
    process: &RcProcess,
    path_ptr: ObjectPointer,
    kind_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;
    let kind = kind_ptr.integer_value()?;
    let dt = filesystem::date_time_for_path(path, kind)?;
    let timestamp = process
        .allocate(object_value::float(dt.timestamp()), state.float_prototype);

    let offset = ObjectPointer::integer(dt.utc_offset());
    let tuple = process.allocate(
        object_value::array(vec![timestamp, offset]),
        state.array_prototype,
    );

    Ok(tuple)
}

#[inline(always)]
pub fn directory_create(
    state: &RcState,
    path_ptr: ObjectPointer,
    recursive_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;

    if is_false!(state, recursive_ptr) {
        fs::create_dir(path)?;
    } else {
        fs::create_dir_all(path)?;
    }

    Ok(state.nil_object)
}

#[inline(always)]
pub fn directory_remove(
    state: &RcState,
    path_ptr: ObjectPointer,
    recursive_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;

    if is_false!(state, recursive_ptr) {
        fs::remove_dir(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }

    Ok(state.nil_object)
}

#[inline(always)]
pub fn directory_list(
    state: &RcState,
    process: &RcProcess,
    path_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let path = path_ptr.string_value()?;
    let files = filesystem::list_directory_as_pointers(&state, process, path)?;

    Ok(files)
}

/// Reads a number of bytes from a stream into a byte array.
fn io_read(
    state: &RcState,
    process: &RcProcess,
    stream: &mut dyn Read,
    buffer: &mut Vec<u8>,
    amount: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let result = if amount.is_integer() {
        let amount_bytes = amount.usize_value()?;

        stream.take(amount_bytes as u64).read_to_end(buffer)?
    } else {
        stream.read_to_end(buffer)?
    };

    // When reading into a buffer, the Vec type may decide to grow it beyond the
    // necessary size. This can lead to a waste of memory, especially when the
    // buffer only sticks around for a short amount of time. To work around this
    // we manually shrink the buffer once we're done writing.
    buffer.shrink_to_fit();

    Ok(process.allocate_usize(result, state.integer_prototype))
}

fn options_for_integer(mode: i64) -> Result<OpenOptions, String> {
    let mut open_opts = OpenOptions::new();

    match mode {
        READ => open_opts.read(true),
        WRITE => open_opts.write(true).truncate(true).create(true),
        APPEND => open_opts.append(true).create(true),
        READ_WRITE => open_opts.read(true).write(true).create(true),
        READ_APPEND => open_opts.read(true).append(true).create(true),
        _ => return Err(format!("Invalid file open mode: {}", mode)),
    };

    Ok(open_opts)
}
