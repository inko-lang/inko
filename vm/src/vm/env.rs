//! VM functions for inspecting and manipulating the OS process' environment.
use directories;
use object_pointer::ObjectPointer;
use object_value;
use platform;
use process::RcProcess;
use std::env;
use std::io::Result as IOResult;
use vm::state::RcState;

/// Returns the value of an environment variable.
pub fn get(
    state: &RcState,
    process: &RcProcess,
    var_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let var_name = var_ptr.string_value()?;

    let val = if let Some(val) = env::var_os(var_name) {
        let string = val.to_string_lossy().into_owned();

        process.allocate(object_value::string(string), state.string_prototype)
    } else {
        state.nil_object
    };

    Ok(val)
}

/// Sets the value of an environment variable.
pub fn set(
    var_ptr: ObjectPointer,
    val_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    env::set_var(var_ptr.string_value()?, val_ptr.string_value()?);

    Ok(val_ptr)
}

/// Removes an environment variable entirely.
pub fn remove(
    state: &RcState,
    var_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    env::remove_var(var_ptr.string_value()?);

    Ok(state.nil_object)
}

/// Returns an array containing all environment variable names.
pub fn names(
    state: &RcState,
    process: &RcProcess,
) -> Result<ObjectPointer, String> {
    let names = env::vars_os()
        .map(|(key, _)| {
            process.allocate(
                object_value::string(key.to_string_lossy().into_owned()),
                state.string_prototype,
            )
        })
        .collect();

    let array =
        process.allocate(object_value::array(names), state.array_prototype);

    Ok(array)
}

/// Returns the home directory of the current user.
pub fn home_directory(
    state: &RcState,
    process: &RcProcess,
) -> Result<ObjectPointer, String> {
    let path = if let Some(path) = directories::home() {
        process.allocate(object_value::string(path), state.string_prototype)
    } else {
        state.nil_object
    };

    Ok(path)
}

/// Returns the temporary directory of the system.
pub fn tmp_directory(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate(
        object_value::string(directories::temp()),
        state.string_prototype,
    )
}

/// Returns the current working directory.
pub fn working_directory(
    state: &RcState,
    process: &RcProcess,
) -> IOResult<ObjectPointer> {
    directories::working_directory().map(|path| {
        process.allocate(object_value::string(path), state.string_prototype)
    })
}

/// Sets the working directory of the current process.
pub fn set_working_directory(
    dir_ptr: ObjectPointer,
) -> Result<IOResult<ObjectPointer>, String> {
    let dir = dir_ptr.string_value()?;

    Ok(directories::set_working_directory(dir).map(|_| dir_ptr))
}

/// Returns all the commandline arguments.
pub fn arguments(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate(
        object_value::array(state.arguments.clone()),
        state.array_prototype,
    )
}

pub fn operating_system(state: &RcState) -> ObjectPointer {
    state.intern_string(platform::operating_system().to_string())
}
