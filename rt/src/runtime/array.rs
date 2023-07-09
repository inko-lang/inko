use crate::mem::{Array, Int, Nil};
use crate::result::Result as InkoResult;
use crate::state::State;

#[no_mangle]
pub unsafe extern "system" fn inko_array_new(
    state: *const State,
    capacity: i64,
) -> *mut Array {
    Array::alloc((*state).array_class, Vec::with_capacity(capacity as _))
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_new_permanent(
    state: *const State,
    capacity: i64,
) -> *mut Array {
    Array::alloc_permanent(
        (*state).array_class,
        Vec::with_capacity(capacity as _),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_reserve(
    array: *mut Array,
    length: i64,
) {
    (*array).value.reserve_exact(length as _);
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
pub unsafe extern "system" fn inko_array_pop(array: *mut Array) -> InkoResult {
    (*array)
        .value
        .pop()
        .map(|v| InkoResult::ok(v as _))
        .unwrap_or_else(InkoResult::none)
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
pub unsafe extern "system" fn inko_array_clear(array: *mut Array) {
    (*array).value.clear();
}

#[no_mangle]
pub unsafe extern "system" fn inko_array_drop(
    state: *const State,
    array: *mut Array,
) -> *const Nil {
    Array::drop(array);
    (*state).nil_singleton
}
