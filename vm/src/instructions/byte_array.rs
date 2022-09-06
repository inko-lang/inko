//! VM functions for working with Inko byte arrays.
use crate::mem::Pointer;
use crate::mem::{ByteArray, Int};
use crate::state::State;

#[inline(always)]
pub(crate) fn allocate(state: &State) -> Pointer {
    ByteArray::alloc(state.permanent_space.byte_array_class(), Vec::new())
}

#[inline(always)]
pub(crate) fn push(array_ptr: Pointer, value_ptr: Pointer) {
    let array = unsafe { array_ptr.get_mut::<ByteArray>() };
    let vector = array.value_mut();
    let value = unsafe { Int::read(value_ptr) } as u8;

    vector.push(value);
}

#[inline(always)]
pub(crate) fn pop(array_ptr: Pointer) -> Pointer {
    let array = unsafe { array_ptr.get_mut::<ByteArray>() };
    let vector = array.value_mut();

    if let Some(value) = vector.pop() {
        Pointer::int(value as i64)
    } else {
        Pointer::undefined_singleton()
    }
}

#[inline(always)]
pub(crate) fn set(
    _: &State,
    array_ptr: Pointer,
    index_ptr: Pointer,
    value_ptr: Pointer,
) -> Pointer {
    unsafe {
        let byte_array = array_ptr.get_mut::<ByteArray>();
        let bytes = byte_array.value_mut();
        let index = Int::read(index_ptr) as usize;
        let index_ref = bytes.get_unchecked_mut(index);
        let old_value = *index_ref;

        *index_ref = Int::read(value_ptr) as u8;
        Pointer::int(old_value as i64)
    }
}

#[inline(always)]
pub(crate) fn get(array_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    unsafe {
        let byte_array = array_ptr.get::<ByteArray>();
        let bytes = byte_array.value();
        let index = Int::read(index_ptr) as usize;

        Pointer::int(*bytes.get_unchecked(index) as i64)
    }
}

#[inline(always)]
pub(crate) fn remove(array_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    let byte_array = unsafe { array_ptr.get_mut::<ByteArray>() };
    let bytes = byte_array.value_mut();
    let index = unsafe { Int::read(index_ptr) as usize };

    Pointer::int(bytes.remove(index) as i64)
}

#[inline(always)]
pub(crate) fn length(state: &State, array_ptr: Pointer) -> Pointer {
    let byte_array = unsafe { array_ptr.get::<ByteArray>() };
    let bytes = byte_array.value();

    Int::alloc(state.permanent_space.int_class(), bytes.len() as i64)
}

#[inline(always)]
pub(crate) fn equals(
    compare_ptr: Pointer,
    compare_with_ptr: Pointer,
) -> Pointer {
    let compare = unsafe { compare_ptr.get::<ByteArray>() };
    let compare_with = unsafe { compare_with_ptr.get::<ByteArray>() };

    if compare.value() == compare_with.value() {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn clear(pointer: Pointer) {
    let bytes = unsafe { pointer.get_mut::<ByteArray>() };

    bytes.value_mut().clear();
}

#[inline(always)]
pub(crate) fn clone(state: &State, pointer: Pointer) -> Pointer {
    let bytes = unsafe { pointer.get_mut::<ByteArray>() }.value().clone();

    ByteArray::alloc(state.permanent_space.byte_array_class(), bytes)
}

#[inline(always)]
pub(crate) fn drop(pointer: Pointer) {
    unsafe {
        ByteArray::drop(pointer);
    }
}
