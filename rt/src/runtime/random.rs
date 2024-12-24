use crate::mem::ByteArray;
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use crate::state::State;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[no_mangle]
pub unsafe extern "system" fn inko_random_int(rng: *mut StdRng) -> i64 {
    (*rng).gen()
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_float(rng: *mut StdRng) -> f64 {
    (*rng).gen()
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_int_range(
    rng: *mut StdRng,
    min: i64,
    max: i64,
) -> i64 {
    if min < max {
        (*rng).gen_range(min..max)
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_random_float_range(
    rng: *mut StdRng,
    min: f64,
    max: f64,
) -> f64 {
    if min < max {
        (*rng).gen_range(min..max)
    } else {
        0.0
    }
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

    ByteArray::alloc((*state).byte_array_type, bytes)
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
pub unsafe extern "system" fn inko_random_drop(rng: *mut StdRng) {
    drop(Box::from_raw(rng));
}
