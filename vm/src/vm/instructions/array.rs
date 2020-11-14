//! VM functions for working with Inko arrays.
use crate::execution_context::ExecutionContext;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::slicing;
use crate::vm::state::RcState;

#[inline(always)]
pub fn array_allocate(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    start_reg: u16,
    amount: u16,
) -> ObjectPointer {
    let mut values = Vec::with_capacity(amount as usize);

    for register in start_reg..(start_reg + amount) {
        values.push(context.get_register(register));
    }

    process.allocate(object_value::array(values), state.array_prototype)
}

#[inline(always)]
pub fn array_set(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let vector = array_ptr.array_value_mut()?;
    let index = slicing::slice_index_to_usize(index_ptr, vector.len())?;

    if index > vector.len() {
        return Err(RuntimeError::out_of_bounds(index));
    }

    let value =
        copy_if_permanent!(state.permanent_allocator, value_ptr, array_ptr);

    if index == vector.len() {
        vector.push(value);
    } else {
        unsafe {
            *vector.get_unchecked_mut(index) = value;
        }
    }

    process.write_barrier(array_ptr, value);
    Ok(value)
}

#[inline(always)]
pub fn array_get(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let vector = array_ptr.array_value()?;
    let index = slicing::slice_index_to_usize(index_ptr, vector.len())?;

    vector
        .get(index)
        .cloned()
        .ok_or_else(|| RuntimeError::out_of_bounds(index))
}

#[inline(always)]
pub fn array_remove(
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let vector = array_ptr.array_value_mut()?;
    let index = slicing::slice_index_to_usize(index_ptr, vector.len())?;

    if index >= vector.len() {
        Err(RuntimeError::out_of_bounds(index))
    } else {
        Ok(vector.remove(index))
    }
}

#[inline(always)]
pub fn array_length(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let vector = array_ptr.array_value()?;

    Ok(process.allocate_usize(vector.len(), state.integer_prototype))
}

#[inline(always)]
pub fn clear(array_ptr: ObjectPointer) -> Result<(), String> {
    array_ptr.array_value_mut()?.clear();
    Ok(())
}
