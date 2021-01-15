//! Functions for setting/getting environment and operating system data.
use crate::directories;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::platform;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use std::env;

/// Gets the value of an environment variable.
///
/// This function requires a single argument: the name of the variable to get.
pub fn env_get(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let var_name = arguments[0].string_value()?;

    if let Some(val) = env::var_os(var_name) {
        let string = val.to_string_lossy().into_owned();

        Ok(process
            .allocate(object_value::string(string), state.string_prototype))
    } else {
        Err(RuntimeError::ErrorMessage(format!(
            "The environment variable {:?} isn't set",
            var_name
        )))
    }
}

/// Sets the value of an environment variable.
///
/// This function requires the following arguments:
///
/// 1. The name of the variable to set.
/// 2. The value to set the variable to.
pub fn env_set(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let val_ptr = arguments[1];

    env::set_var(arguments[0].string_value()?, val_ptr.string_value()?);
    Ok(val_ptr)
}

/// Removes an environment variable.
///
/// This function requires one argument: the name of the variable to remove.
pub fn env_remove(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    env::remove_var(arguments[0].string_value()?);
    Ok(state.nil_object)
}

/// Returns an Array containing all environment variable names.
///
/// This function doesn't take any arguments.
pub fn env_variables(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
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

/// Returns the user's home directory.
pub fn env_home_directory(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    if let Some(path) = directories::home() {
        Ok(
            process
                .allocate(object_value::string(path), state.string_prototype),
        )
    } else {
        Err(RuntimeError::Error(state.nil_object))
    }
}

/// Returns the temporary directory.
pub fn env_temp_directory(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let res = process.allocate(
        object_value::string(directories::temp()),
        state.string_prototype,
    );

    Ok(res)
}

/// Returns the current working directory.
pub fn env_get_working_directory(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let path = directories::working_directory()?;

    Ok(process.allocate(object_value::string(path), state.string_prototype))
}

/// Sets the working directory.
///
/// This function requires one argument: the path of the new directory.
pub fn env_set_working_directory(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let dir_ptr = arguments[0];
    let dir = dir_ptr.string_value()?;

    directories::set_working_directory(dir)?;
    Ok(dir_ptr)
}

/// Returns the commandline arguments.
pub fn env_arguments(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let res = process.allocate(
        object_value::array(state.arguments.clone()),
        state.array_prototype,
    );

    Ok(res)
}

/// Returns the name of the underlying platform.
pub fn env_platform_name(
    state: &RcState,
    _: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(state.intern_string(platform::operating_system().to_string()))
}

register!(
    env_get,
    env_set,
    env_remove,
    env_variables,
    env_home_directory,
    env_temp_directory,
    env_get_working_directory,
    env_set_working_directory,
    env_arguments,
    env_platform_name
);
