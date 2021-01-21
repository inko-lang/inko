//! Functions for working with Inko strings.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::slicing;
use crate::vm::state::RcState;
use num_bigint::BigInt;

/// Converts a String to lowercase.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_lower(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let lower = arguments[0].string_value()?.to_lowercase();

    Ok(process.allocate(
        object_value::immutable_string(lower),
        state.string_prototype,
    ))
}

/// Converts a String to lowercase.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_upper(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let upper = arguments[0].string_value()?.to_uppercase();

    Ok(process.allocate(
        object_value::immutable_string(upper),
        state.string_prototype,
    ))
}

/// Converts a String to a ByteArray.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_byte_array(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let bytes = arguments[0].string_value()?.as_bytes().to_vec();
    let value = object_value::byte_array(bytes);

    Ok(process.allocate(value, state.byte_array_prototype))
}

/// Concatenates multiple Strings together.
///
/// This function requires a single argument: an array of strings to
/// concatenate.
pub fn string_concat_array(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let array = arguments[0].array_value()?;
    let mut buffer = String::new();

    for str_ptr in array.iter() {
        buffer.push_str(str_ptr.string_value()?.as_slice());
    }

    Ok(process.allocate(object_value::string(buffer), state.string_prototype))
}

/// Formats a String for debugging purposes.
///
/// This function requires a single argument: the string to format.
pub fn string_format_debug(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let new_str = format!("{:?}", arguments[0].string_value()?);

    Ok(process.allocate(object_value::string(new_str), state.string_prototype))
}

/// Slices a String into a new String.
///
/// This function requires the following arguments:
///
/// 1. The String to slice.
/// 2. The start position of the slice.
/// 3. The number of characters to include in the new slice.
pub fn string_slice(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let string = arguments[0].string_value()?;
    let start =
        slicing::slice_index_to_usize(arguments[1], string.chars().count())?;

    let amount = arguments[2].usize_value()?;
    let new_string =
        string.chars().skip(start).take(amount).collect::<String>();

    let new_string_ptr = process
        .allocate(object_value::string(new_string), state.string_prototype);

    Ok(new_string_ptr)
}

/// Converts a String to an integer.
///
/// This function requires the following arguments:
///
/// 1. The String to convert.
/// 2. The radix to use for converting the String to an Integer.
pub fn string_to_integer(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let string = arguments[0].string_value()?;
    let radix = arguments[1].integer_value()?;

    if !(2..=36).contains(&radix) {
        return Err(RuntimeError::Panic(
            "radix must be between 2 and 32, not {}".to_string(),
        ));
    }

    let int_ptr = if let Ok(value) = i64::from_str_radix(string, radix as u32) {
        process.allocate_i64(value, state.integer_prototype)
    } else if let Ok(val) = string.parse::<BigInt>() {
        process.allocate(object_value::bigint(val), state.integer_prototype)
    } else {
        return Err(RuntimeError::ErrorMessage(format!(
            "{:?} can not be converted to an Integer",
            string
        )));
    };

    Ok(int_ptr)
}

/// Converts a String to a Float.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_float(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let string = arguments[0].string_value()?;

    if let Ok(value) = string.parse::<f64>() {
        let pointer =
            process.allocate(object_value::float(value), state.float_prototype);

        Ok(pointer)
    } else {
        Err(RuntimeError::ErrorMessage(format!(
            "{:?} can not be converted to a Float",
            string
        )))
    }
}

register!(
    string_to_lower,
    string_to_upper,
    string_to_byte_array,
    string_concat_array,
    string_format_debug,
    string_slice,
    string_to_integer,
    string_to_float
);
