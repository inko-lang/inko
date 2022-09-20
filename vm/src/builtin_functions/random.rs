//! Functions for generating random numbers.
use crate::mem::{ByteArray, Float, Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub(crate) fn random_int(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let rng = unsafe { arguments[0].get_mut::<StdRng>() };

    Ok(Int::alloc(state.permanent_space.int_class(), rng.gen()))
}

pub(crate) fn random_float(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let rng = unsafe { arguments[0].get_mut::<StdRng>() };

    Ok(Float::alloc(state.permanent_space.float_class(), rng.gen()))
}

pub(crate) fn random_int_range(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let rng = unsafe { arguments[0].get_mut::<StdRng>() };
    let min = unsafe { Int::read(arguments[1]) };
    let max = unsafe { Int::read(arguments[2]) };
    let val = if min < max { rng.gen_range(min..max) } else { 0 };

    Ok(Int::alloc(state.permanent_space.int_class(), val))
}

pub(crate) fn random_float_range(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let rng = unsafe { arguments[0].get_mut::<StdRng>() };
    let min = unsafe { Float::read(arguments[1]) };
    let max = unsafe { Float::read(arguments[2]) };
    let val = if min < max { rng.gen_range(min..max) } else { 0.0 };

    Ok(Float::alloc(state.permanent_space.float_class(), val))
}

pub(crate) fn random_bytes(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let rng = unsafe { arguments[0].get_mut::<StdRng>() };
    let size = unsafe { Int::read(arguments[1]) } as usize;
    let mut bytes = vec![0; size];

    rng.try_fill(&mut bytes[..]).map_err(|e| e.to_string())?;
    Ok(ByteArray::alloc(state.permanent_space.byte_array_class(), bytes))
}

pub(crate) fn random_new(
    _: &State,
    thread: &mut Thread,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut seed: <StdRng as SeedableRng>::Seed = Default::default();

    thread.rng.fill(&mut seed);

    let rng = Pointer::boxed(StdRng::from_seed(seed));

    Ok(rng)
}

pub(crate) fn random_from_int(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let seed = unsafe { Int::read(args[0]) } as u64;
    let rng = Pointer::boxed(StdRng::seed_from_u64(seed));

    Ok(rng)
}

pub(crate) fn random_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { args[0].drop_boxed::<StdRng>() };
    Ok(Pointer::nil_singleton())
}
