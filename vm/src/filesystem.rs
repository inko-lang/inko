//! Helpers for working with the filesystem.

use date_time::DateTime;
use error_messages;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use std::fs;
use vm::state::RcState;

const TIME_CREATED: i64 = 0;
const TIME_MODIFIED: i64 = 1;
const TIME_ACCESSED: i64 = 2;

const TYPE_INVALID: i64 = 0;
const TYPE_FILE: i64 = 1;
const TYPE_DIRECTORY: i64 = 2;

macro_rules! map_io {
    ($op:expr) => {{
        $op.map_err(|err| error_messages::from_io_error(err))
    }};
}

/// Returns a DateTime for the given path.
///
/// The `kind` argument specifies whether the creation, modification or access
/// time should be retrieved.
pub fn date_time_for_path(
    path: &String,
    kind: i64,
) -> Result<DateTime, String> {
    let meta = map_io!(fs::metadata(path))?;

    let system_time = match kind {
        TIME_CREATED => map_io!(meta.created())?,
        TIME_MODIFIED => map_io!(meta.modified())?,
        TIME_ACCESSED => map_io!(meta.accessed())?,
        _ => return Err(format!("{} is not a valid type of timestamp", kind)),
    };

    Ok(DateTime::from_system_time(system_time))
}

/// Returns the type of the given path.
pub fn type_of_path(path: &String) -> i64 {
    if let Ok(meta) = map_io!(fs::metadata(path)) {
        if meta.is_dir() {
            TYPE_DIRECTORY
        } else {
            TYPE_FILE
        }
    } else {
        TYPE_INVALID
    }
}

/// Returns an Array containing the contents of a directory.
///
/// The entries are allocated right away so no additional mapping of vectors is
/// necessary.
pub fn list_directory_as_pointers(
    state: &RcState,
    process: &RcProcess,
    path: &String,
) -> Result<ObjectPointer, String> {
    let mut paths = Vec::new();

    for entry in map_io!(fs::read_dir(path))? {
        let entry = map_io!(entry)?;
        let path = entry.path().to_string_lossy().to_string();
        let pointer = process
            .allocate(object_value::string(path), state.string_prototype);

        paths.push(pointer);
    }

    let paths_ptr =
        process.allocate(object_value::array(paths), state.array_prototype);

    Ok(paths_ptr)
}
