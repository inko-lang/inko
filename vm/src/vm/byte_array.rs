//! VM functions for working with Inko byte arrays.
use crate::immutable_string::ImmutableString;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::slicing;
use crate::vm::state::RcState;
use std::u8;

const MIN_BYTE: i64 = u8::MIN as i64;
const MAX_BYTE: i64 = u8::MAX as i64;

/// Converts a tagged integer to a u8, if possible.
pub fn integer_to_byte(pointer: ObjectPointer) -> Result<u8, String> {
    let value = pointer.integer_value()?;

    if value >= MIN_BYTE && value <= MAX_BYTE {
        Ok(value as u8)
    } else {
        Err(format!(
            "The value {} is not within the range 0..256",
            value
        ))
    }
}

pub fn create(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let integers = array_ptr.array_value()?;
    let mut bytes = Vec::with_capacity(integers.len());

    for value in integers.iter() {
        bytes.push(integer_to_byte(*value)?);
    }

    Ok(process
        .allocate(object_value::byte_array(bytes), state.byte_array_prototype))
}

pub fn set(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = array_ptr.byte_array_value_mut()?;
    let index =
        slicing::index_for_slice(bytes.len(), index_ptr.integer_value()?);

    let value = integer_to_byte(value_ptr)?;

    if index > bytes.len() {
        return Err(format!("Byte array index {} is out of bounds", index));
    }

    if index == bytes.len() {
        bytes.push(value);
    } else {
        bytes[index] = value;
    }

    Ok(value_ptr)
}

pub fn get(
    state: &RcState,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = array_ptr.byte_array_value()?;

    let index =
        slicing::index_for_slice(bytes.len(), index_ptr.integer_value()?);

    let value = bytes
        .get(index)
        .map(|byte| ObjectPointer::byte(*byte))
        .unwrap_or_else(|| state.nil_object);

    Ok(value)
}

pub fn remove(
    state: &RcState,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = array_ptr.byte_array_value_mut()?;
    let index =
        slicing::index_for_slice(bytes.len(), index_ptr.integer_value()?);

    let value = if index >= bytes.len() {
        state.nil_object
    } else {
        ObjectPointer::byte(bytes.remove(index))
    };

    Ok(value)
}

pub fn length(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = array_ptr.byte_array_value()?;

    Ok(process.allocate_usize(bytes.len(), state.integer_prototype))
}

pub fn clear(array_ptr: ObjectPointer) -> Result<(), String> {
    array_ptr.byte_array_value_mut()?.clear();

    Ok(())
}

pub fn equals(
    state: &RcState,
    compare_ptr: ObjectPointer,
    compare_with_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = if compare_ptr.byte_array_value()?
        == compare_with_ptr.byte_array_value()?
    {
        state.true_object
    } else {
        state.false_object
    };

    Ok(result)
}

pub fn to_string(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
    drain_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let input_bytes = array_ptr.byte_array_value_mut()?;

    let string_bytes = if drain_ptr == state.true_object {
        input_bytes.drain(0..).collect()
    } else {
        input_bytes.clone()
    };

    let string = ImmutableString::from_utf8(string_bytes);

    Ok(process.allocate(
        object_value::immutable_string(string),
        state.string_prototype,
    ))
}
