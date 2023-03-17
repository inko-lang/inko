use crate::mem::{Array, Int, Nil};
use crate::state::State;
use std::ptr::null_mut;

#[no_mangle]
pub unsafe extern "system" fn inko_array_new(
    state: *const State,
    length: usize,
) -> *mut Array {
    Array::alloc((*state).array_class, Vec::with_capacity(length))
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_new_permanent(
    state: *const State,
    length: usize,
) -> *mut Array {
    Array::alloc_permanent((*state).array_class, Vec::with_capacity(length))
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_reserve(
    state: *const State,
    array: *mut Array,
    length: usize,
) -> *const Nil {
    (*array).value.reserve_exact(length);
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_push(
    state: *const State,
    array: *mut Array,
    value: *mut u8,
) -> *const Nil {
    (*array).value.push(value);
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_pop(array: *mut Array) -> *mut u8 {
    (*array).value.pop().unwrap_or_else(null_mut)
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_set(
    array: *mut Array,
    index: i64,
    value: *mut u8,
) -> *mut u8 {
    let array = &mut *array;
    let index_ref = array.value.get_unchecked_mut(index as usize);
    let old_value = *index_ref;

    *index_ref = value;
    old_value
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_get(
    array: *const Array,
    index: i64,
) -> *mut u8 {
    *(*array).value.get_unchecked(index as usize)
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_remove(
    array: *mut Array,
    index: i64,
) -> *mut u8 {
    (*array).value.remove(index as usize)
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_length(
    state: *const State,
    array: *const Array,
) -> *const Int {
    Int::new((*state).int_class, (*array).value.len() as i64)
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_capacity(
    state: *const State,
    array: *const Array,
) -> *const Int {
    Int::new((*state).int_class, (*array).value.capacity() as i64)
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_clear(
    state: *const State,
    array: *mut Array,
) -> *const Nil {
    (*array).value.clear();
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_drop(
    state: *const State,
    array: *mut Array,
) -> *const Nil {
    Array::drop(array);
    (*state).nil_singleton
}
