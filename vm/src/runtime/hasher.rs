use crate::hasher::Hasher;
use crate::mem::{Int, Nil};
use crate::state::State;
use std::hash::BuildHasher as _;

#[no_mangle]
pub unsafe extern "system" fn inko_hasher_new(state: &State) -> *mut Hasher {
    let hasher = (*state).hash_state.build_hasher();

    Box::into_raw(Box::new(Hasher::new(hasher)))
}

#[no_mangle]
pub unsafe extern "system" fn inko_hasher_write_int(
    state: *const State,
    hasher: *mut Hasher,
    value: *const Int,
) -> *const Nil {
    (*hasher).write_int(Int::read(value));
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_hasher_to_hash(
    state: *const State,
    hasher: *mut Hasher,
) -> *const Int {
    Int::new((*state).int_class, (*hasher).finish())
}

#[no_mangle]
pub unsafe extern "system" fn inko_hasher_drop(
    state: *const State,
    hasher: *mut Hasher,
) -> *const Nil {
    drop(Box::from_raw(hasher));
    (*state).nil_singleton
}
