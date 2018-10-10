use object_pointer::ObjectPointer;
use std::fs::OpenOptions;
use vm::state::RcState;

/// File opened for reading, equal to fopen's "r" mode.
pub const READ: i64 = 0;

/// File opened for writing, equal to fopen's "w" mode.
pub const WRITE: i64 = 1;

/// File opened for appending, equal to fopen's "a" mode.
pub const APPEND: i64 = 2;

/// File opened for both reading and writing, equal to fopen's "w+" mode.
pub const READ_WRITE: i64 = 3;

/// File opened for reading and appending, equal to fopen's "a+" mode.
pub const READ_APPEND: i64 = 4;

macro_rules! file_mode_error {
    ($mode: expr) => {
        return Err(format!("Invalid file open mode: {}", $mode));
    };
}

pub fn options_for_integer(mode: i64) -> Result<OpenOptions, String> {
    let mut open_opts = OpenOptions::new();

    match mode {
        READ => {
            open_opts.read(true);
        }
        WRITE => {
            open_opts.write(true).truncate(true).create(true);
        }
        APPEND => {
            open_opts.append(true).create(true);
        }
        READ_WRITE => {
            open_opts.read(true).write(true).create(true);
        }
        READ_APPEND => {
            open_opts.read(true).append(true).create(true);
        }
        _ => file_mode_error!(mode),
    };

    Ok(open_opts)
}

pub fn prototype_for_open_mode(
    state: &RcState,
    mode: i64,
) -> Result<ObjectPointer, String> {
    let proto = match mode {
        READ => state.read_only_file_prototype,
        WRITE | APPEND => state.write_only_file_prototype,
        READ_WRITE | READ_APPEND => state.read_write_file_prototype,
        _ => file_mode_error!(mode),
    };

    Ok(proto)
}
