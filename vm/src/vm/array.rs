//! VM functions for working with Inko arrays.
use crate::execution_context::ExecutionContext;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::slicing;
use crate::vm::state::RcState;

// Creates a new array, populated with the values of the given registers.
pub fn create(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    registers: &[u16],
) -> ObjectPointer {
    let values = registers
        .iter()
        .map(|reg| context.get_register(*reg as usize))
        .collect();

    process.allocate(object_value::array(values), state.array_prototype)
}

pub fn set(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let vector = array_ptr.array_value_mut()?;
    let index =
        slicing::index_for_slice(vector.len(), index_ptr.integer_value()?);

    let value =
        copy_if_permanent!(state.permanent_allocator, value_ptr, array_ptr);

    if index >= vector.len() {
        vector.resize(index + 1, state.nil_object);
    }

    vector[index] = value;

    process.write_barrier(array_ptr, value);

    Ok(value)
}

pub fn get(
    state: &RcState,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let vector = array_ptr.array_value()?;

    let index =
        slicing::index_for_slice(vector.len(), index_ptr.integer_value()?);

    let value = vector
        .get(index)
        .cloned()
        .unwrap_or_else(|| state.nil_object);

    Ok(value)
}

pub fn remove(
    state: &RcState,
    array_ptr: ObjectPointer,
    index_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let vector = array_ptr.array_value_mut()?;

    let index =
        slicing::index_for_slice(vector.len(), index_ptr.integer_value()?);

    let value = if index >= vector.len() {
        state.nil_object
    } else {
        vector.remove(index)
    };

    Ok(value)
}

pub fn length(
    state: &RcState,
    process: &RcProcess,
    array_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let vector = array_ptr.array_value()?;

    Ok(process.allocate_usize(vector.len(), state.integer_prototype))
}

pub fn clear(array_ptr: ObjectPointer) -> Result<(), String> {
    array_ptr.array_value_mut()?.clear();

    Ok(())
}
