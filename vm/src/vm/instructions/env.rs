//! VM functions for inspecting and manipulating the OS process' environment.
use crate::directories;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::platform;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use std::env;

#[inline(always)]
pub fn env_get(
    state: &RcState,
    process: &RcProcess,
    var_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let var_name = var_ptr.string_value()?;

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

#[inline(always)]
pub fn env_set(
    var_ptr: ObjectPointer,
    val_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    env::set_var(var_ptr.string_value()?, val_ptr.string_value()?);

    Ok(val_ptr)
}

#[inline(always)]
pub fn env_remove(var_ptr: ObjectPointer) -> Result<(), String> {
    env::remove_var(var_ptr.string_value()?);
    Ok(())
}

#[inline(always)]
pub fn env_variables(
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

#[inline(always)]
pub fn env_home_directory(
    state: &RcState,
    process: &RcProcess,
) -> Result<ObjectPointer, RuntimeError> {
    if let Some(path) = directories::home() {
        Ok(
            process
                .allocate(object_value::string(path), state.string_prototype),
        )
    } else {
        Err(RuntimeError::ErrorMessage(
            "The user's home directory isn't set".to_string(),
        ))
    }
}

#[inline(always)]
pub fn env_temp_directory(
    state: &RcState,
    process: &RcProcess,
) -> ObjectPointer {
    process.allocate(
        object_value::string(directories::temp()),
        state.string_prototype,
    )
}

#[inline(always)]
pub fn env_get_working_directory(
    state: &RcState,
    process: &RcProcess,
) -> Result<ObjectPointer, RuntimeError> {
    let path = directories::working_directory()?;

    Ok(process.allocate(object_value::string(path), state.string_prototype))
}

#[inline(always)]
pub fn env_set_working_directory(
    dir_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let dir = dir_ptr.string_value()?;

    directories::set_working_directory(dir)?;

    Ok(dir_ptr)
}

#[inline(always)]
pub fn env_arguments(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process.allocate(
        object_value::array(state.arguments.clone()),
        state.array_prototype,
    )
}

#[inline(always)]
pub fn platform(state: &RcState) -> ObjectPointer {
    state.intern_string(platform::operating_system().to_string())
}
