//! VM functions for working with Inko strings.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::slicing;
use crate::vm::state::RcState;
use num_bigint::BigInt;

pub fn to_lower(
    state: &RcState,
    process: &RcProcess,
    string_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let lower = string_ptr.string_value()?.to_lowercase();

    Ok(process.allocate(
        object_value::immutable_string(lower),
        state.string_prototype,
    ))
}

pub fn to_upper(
    state: &RcState,
    process: &RcProcess,
    string_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let upper = string_ptr.string_value()?.to_uppercase();

    Ok(process.allocate(
        object_value::immutable_string(upper),
        state.string_prototype,
    ))
}

pub fn equal(
    state: &RcState,
    compare: ObjectPointer,
    compare_with: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let boolean =
        if compare.is_interned_string() && compare_with.is_interned_string() {
            if compare == compare_with {
                state.true_object
            } else {
                state.false_object
            }
        } else if compare.string_value()? == compare_with.string_value()? {
            state.true_object
        } else {
            state.false_object
        };

    Ok(boolean)
}

pub fn to_byte_array(
    state: &RcState,
    process: &RcProcess,
    string: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = string.string_value()?.as_bytes().to_vec();
    let value = object_value::byte_array(bytes);

    Ok(process.allocate(value, state.byte_array_prototype))
}

pub fn length(
    state: &RcState,
    process: &RcProcess,
    string: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let length = process.allocate_usize(
        string.string_value()?.chars().count(),
        state.integer_prototype,
    );

    Ok(length)
}

pub fn byte_size(
    state: &RcState,
    process: &RcProcess,
    string: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let size = process
        .allocate_usize(string.string_value()?.len(), state.integer_prototype);

    Ok(size)
}

pub fn concat(
    state: &RcState,
    process: &RcProcess,
    concat: ObjectPointer,
    concat_with: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let new_string = concat.string_value()? + concat_with.string_value()?;

    let result = process.allocate(
        object_value::immutable_string(new_string),
        state.string_prototype,
    );

    Ok(result)
}

pub fn concat_multiple(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let array = array_ptr.array_value()?;
    let mut buffer = String::new();

    for str_ptr in array.iter() {
        buffer.push_str(str_ptr.string_value()?.as_slice());
    }

    Ok(process.allocate(object_value::string(buffer), state.string_prototype))
}

pub fn slice(
    state: &RcState,
    process: &RcProcess,
    str_ptr: ObjectPointer,
    start_ptr: ObjectPointer,
    amount_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let string = str_ptr.string_value()?;
    let amount = amount_ptr.usize_value()?;

    let start = slicing::index_for_slice(
        string.chars().count(),
        start_ptr.integer_value()?,
    );

    let new_string =
        string.chars().skip(start).take(amount).collect::<String>();

    let new_string_ptr = process
        .allocate(object_value::string(new_string), state.string_prototype);

    Ok(new_string_ptr)
}

pub fn format_debug(
    state: &RcState,
    process: &RcProcess,
    str_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let new_str = format!("{:?}", str_ptr.string_value()?);

    Ok(process.allocate(object_value::string(new_str), state.string_prototype))
}

/// Converts a string to an integer.
pub fn to_integer(
    state: &RcState,
    process: &RcProcess,
    str_ptr: ObjectPointer,
    radix_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let string = str_ptr.string_value()?;
    let radix = radix_ptr.integer_value()?;

    if radix < 2 || radix > 36 {
        return Err(RuntimeError::Panic(
            "radix must be between 2 and 32, not {}".to_string(),
        ));
    }

    let int_ptr = if let Ok(value) = i64::from_str_radix(string, radix as u32) {
        process.allocate_i64(value, state.integer_prototype)
    } else if let Ok(val) = string.parse::<BigInt>() {
        process.allocate(object_value::bigint(val), state.integer_prototype)
    } else {
        return Err(RuntimeError::Exception(format!(
            "{:?} can not be converted to an Integer",
            string
        )));
    };

    Ok(int_ptr)
}

/// Converts a string to a float.
pub fn to_float(
    state: &RcState,
    process: &RcProcess,
    str_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let string = str_ptr.string_value()?;

    if let Ok(value) = string.parse::<f64>() {
        let pointer =
            process.allocate(object_value::float(value), state.float_prototype);

        Ok(pointer)
    } else {
        Err(RuntimeError::Exception(format!(
            "{:?} can not be converted to a Float",
            string
        )))
    }
}
