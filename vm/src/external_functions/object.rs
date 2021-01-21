//! Functions for working with Inko objects.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Returns the names of an object's attributes.
///
/// This function requires one argument: the object to get the attribute names
/// of.
pub fn object_attribute_names(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let names = arguments[0].attribute_names();

    Ok(process.allocate(object_value::array(names), state.array_prototype))
}

register!(object_attribute_names);
