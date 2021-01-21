//! Functions for working with Inko arrays.
use crate::object_pointer::ObjectPointer;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Removes all values from an array.
///
/// This function requires a single argument: the array to clear.
pub fn array_clear(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].array_value_mut()?.clear();
    Ok(state.nil_object)
}

register!(array_clear);
