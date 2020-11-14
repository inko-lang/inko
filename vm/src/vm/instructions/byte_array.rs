//! VM functions for working with Inko byte arrays.
use crate::immutable_string::ImmutableString;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::slicing;
use crate::vm::state::RcState;
use std::u8;

const MIN_BYTE: i64 = u8::MIN as i64;
const MAX_BYTE: i64 = u8::MAX as i64;

#[inline(always)]
pub fn byte_array_from_array(
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

#[inline(always)]
pub fn byte_array_set(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let bytes = array_ptr.byte_array_value_mut()?;
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len())?;
    let value = integer_to_byte(value_ptr)?;

    if index > bytes.len() {
        return Err(RuntimeError::out_of_bounds(index));
    }

    if index == bytes.len() {
        bytes.push(value);
    } else {
        unsafe {
            *bytes.get_unchecked_mut(index) = value;
        }
    }

    Ok(value_ptr)
}

#[inline(always)]
pub fn byte_array_get(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let bytes = array_ptr.byte_array_value()?;
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len())?;

    bytes
        .get(index)
        .map(|byte| ObjectPointer::byte(*byte))
        .ok_or_else(|| RuntimeError::out_of_bounds(index))
}

#[inline(always)]
pub fn byte_array_remove(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let bytes = array_ptr.byte_array_value_mut()?;
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len())?;

    if index >= bytes.len() {
        Err(RuntimeError::out_of_bounds(index))
    } else {
        Ok(ObjectPointer::byte(bytes.remove(index)))
    }
}

#[inline(always)]
pub fn byte_array_length(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bytes = array_ptr.byte_array_value()?;

    Ok(process.allocate_usize(bytes.len(), state.integer_prototype))
}

#[inline(always)]
pub fn byte_array_clear(array_ptr: ObjectPointer) -> Result<(), String> {
    array_ptr.byte_array_value_mut()?.clear();

    Ok(())
}

#[inline(always)]
pub fn byte_array_equals(
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

#[inline(always)]
pub fn byte_array_to_string(
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

fn integer_to_byte(pointer: ObjectPointer) -> Result<u8, String> {
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
