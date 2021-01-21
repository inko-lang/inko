//! Functions for working with Inko integers.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use num_traits::ToPrimitive;

/// Converts an Integer to a Float.
///
/// This function requires a single argument: the integer to convert.
pub fn integer_to_float(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let integer = arguments[0];
    let result = if integer.is_bigint() {
        let bigint = integer.bigint_value().unwrap();

        if let Some(float) = bigint.to_f64() {
            float
        } else {
            return Err(
                format!("Failed to convert {} to a float", bigint).into()
            );
        }
    } else {
        integer.integer_value()? as f64
    };

    Ok(process.allocate(object_value::float(result), state.float_prototype))
}

/// Converts an Integer to a String.
///
/// This function requires a single argument: the integer to convert.
pub fn integer_to_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let integer = arguments[0];
    let result = if integer.is_integer() {
        integer.integer_value()?.to_string()
    } else if integer.is_bigint() {
        integer.bigint_value()?.to_string()
    } else {
        return Err("The Integer can't be converted to a String".into());
    };

    Ok(process.allocate(object_value::string(result), state.string_prototype))
}

register!(integer_to_float, integer_to_string);
