//! Functions for working with Inko arrays.
use crate::immutable_string::ImmutableString;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Removes all values from a ByteArray.
///
/// This function requires a single argument: the ByteArray to clear.
pub fn byte_array_clear(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    arguments[0].byte_array_value_mut()?.clear();
    Ok(state.nil_object)
}

/// Converts a ByteArray to a String.
///
/// This function requires a single argument: the ByteArray to convert.
pub fn byte_array_to_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input_bytes = arguments[0].byte_array_value()?;
    let string = ImmutableString::from_utf8(input_bytes.clone());

    Ok(process.allocate(
        object_value::immutable_string(string),
        state.string_prototype,
    ))
}

/// Converts a ByteArray to a String by draining the input ByteArray.
///
/// This function requires a single argument: the ByteArray to convert.
pub fn byte_array_drain_to_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let input_bytes = arguments[0].byte_array_value_mut()?;
    let string = ImmutableString::from_utf8(input_bytes.drain(0..).collect());

    Ok(process.allocate(
        object_value::immutable_string(string),
        state.string_prototype,
    ))
}

register!(
    byte_array_clear,
    byte_array_to_string,
    byte_array_drain_to_string
);
