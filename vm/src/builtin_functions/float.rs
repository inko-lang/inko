use crate::mem::{Float, Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

pub(crate) fn float_to_bits(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bits = unsafe { Float::read(args[0]) }.to_bits() as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), bits))
}

pub(crate) fn float_from_bits(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bits = unsafe { Int::read(args[0]) } as u64;

    Ok(Float::alloc(state.permanent_space.float_class(), f64::from_bits(bits)))
}
