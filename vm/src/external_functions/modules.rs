//! Functions for working with Inko modules.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Returns all the modules that have been defined.
pub fn module_list(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let modules = state.modules.lock().list();

    Ok(process.allocate(object_value::array(modules), state.array_prototype))
}

/// Returns the name of a module.
///
/// This function requires a single argument: the module to get the data from.
pub fn module_name(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(arguments[0].module_value()?.name())
}

/// Returns the source path of a module.
///
/// This function requires a single argument: the module to get the data from.
pub fn module_source_path(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(arguments[0].module_value()?.source_path())
}

register!(module_list, module_name, module_source_path);
