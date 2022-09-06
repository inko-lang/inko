//! VM functions for working with Inko arrays.
use crate::mem::Pointer;
use crate::mem::{Array, Int};
use crate::process::TaskPointer;
use crate::state::State;

#[inline(always)]
pub(crate) fn allocate(state: &State, mut task: TaskPointer) -> Pointer {
    // We use this approach so our array's capacity isn't dictated by the
    // stack's capacity.
    let mut values = Vec::with_capacity(task.stack.len());

    values.append(&mut task.stack);
    Array::alloc(state.permanent_space.array_class(), values)
}

#[inline(always)]
pub(crate) fn push(array_ptr: Pointer, value_ptr: Pointer) {
    let array = unsafe { array_ptr.get_mut::<Array>() };
    let vector = array.value_mut();

    vector.push(value_ptr);
}

#[inline(always)]
pub(crate) fn pop(array_ptr: Pointer) -> Pointer {
    let array = unsafe { array_ptr.get_mut::<Array>() };
    let vector = array.value_mut();

    vector.pop().unwrap_or_else(Pointer::undefined_singleton)
}

#[inline(always)]
pub(crate) fn set(
    array_ptr: Pointer,
    index_ptr: Pointer,
    value_ptr: Pointer,
) -> Pointer {
    unsafe {
        let array = array_ptr.get_mut::<Array>();
        let vector = array.value_mut();
        let index = Int::read(index_ptr) as usize;
        let index_ref = vector.get_unchecked_mut(index);
        let old_value = *index_ref;

        *index_ref = value_ptr;
        old_value
    }
}

#[inline(always)]
pub(crate) fn get(array_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    unsafe {
        let array = array_ptr.get::<Array>();
        let vector = array.value();
        let index = Int::read(index_ptr) as usize;

        *vector.get_unchecked(index)
    }
}

#[inline(always)]
pub(crate) fn remove(array_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    let array = unsafe { array_ptr.get_mut::<Array>() };
    let vector = array.value_mut();
    let index = unsafe { Int::read(index_ptr) as usize };

    vector.remove(index)
}

#[inline(always)]
pub(crate) fn length(state: &State, pointer: Pointer) -> Pointer {
    let array = unsafe { pointer.get::<Array>() };
    let vector = array.value();

    Int::alloc(state.permanent_space.int_class(), vector.len() as i64)
}

#[inline(always)]
pub(crate) fn clear(array_ptr: Pointer) {
    unsafe { array_ptr.get_mut::<Array>() }.value_mut().clear();
}

#[inline(always)]
pub(crate) fn drop(array_ptr: Pointer) {
    unsafe {
        Array::drop(array_ptr);
    }
}
