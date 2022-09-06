//! Functions for hashing objects.
use crate::hasher::Hasher;
use crate::mem::{Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;
use std::hash::BuildHasher as _;

pub(crate) fn hasher_new(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = state.hash_state.build_hasher();

    Ok(Pointer::boxed(Hasher::new(hasher)))
}

pub(crate) fn hasher_write_int(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = unsafe { Int::read(arguments[1]) };

    hasher.write_int(value);
    Ok(Pointer::nil_singleton())
}

pub(crate) fn hasher_to_hash(
    state: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = hasher.finish();

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

pub(crate) fn hasher_drop(
    _: &State,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Hasher>();
    }

    Ok(Pointer::nil_singleton())
}
