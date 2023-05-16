use crate::mem::{ByteArray, Float, Int, Nil};
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use crate::state::State;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[no_mangle]
pub unsafe extern "system" fn inko_random_int(
    state: *const State,
    rng: *mut StdRng,
) -> *const Int {
    Int::new((*state).int_class, (*rng).gen())
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_float(
    state: *const State,
    rng: *mut StdRng,
) -> *const Float {
    Float::alloc((*state).float_class, (*rng).gen())
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_int_range(
    state: *const State,
    rng: *mut StdRng,
    min: i64,
    max: i64,
) -> *const Int {
    let val = if min < max { (*rng).gen_range(min..max) } else { 0 };

    Int::new((*state).int_class, val)
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_float_range(
    state: *const State,
    rng: *mut StdRng,
    min: f64,
    max: f64,
) -> *const Float {
    let val = if min < max { (*rng).gen_range(min..max) } else { 0.0 };

    Float::alloc((*state).float_class, val)
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_bytes(
    state: *const State,
    process: ProcessPointer,
    rng: *mut StdRng,
    size: i64,
) -> *mut ByteArray {
    let mut bytes = vec![0; size as usize];

    if let Err(err) = (*rng).try_fill(&mut bytes[..]) {
        panic(process, &err.to_string());
    }

    ByteArray::alloc((*state).byte_array_class, bytes)
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_new(
    mut process: ProcessPointer,
) -> *mut StdRng {
    let mut seed: <StdRng as SeedableRng>::Seed = Default::default();

    process.thread().rng.fill(&mut seed);
    Box::into_raw(Box::new(StdRng::from_seed(seed)))
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_from_int(seed: i64) -> *mut StdRng {
    Box::into_raw(Box::new(StdRng::seed_from_u64(seed as _)))
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_drop(
    state: *const State,
    rng: *mut StdRng,
) -> *const Nil {
    drop(Box::from_raw(rng));
    (*state).nil_singleton
}
