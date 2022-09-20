use crate::mem::{Array, Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

pub(crate) fn array_reserve(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { args[0].get_mut::<Array>() };
    let size = unsafe { Int::read(args[1]) };

    array.value_mut().reserve(size as usize);
    Ok(Pointer::nil_singleton())
}

pub(crate) fn array_capacity(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { args[0].get::<Array>() };
    let cap = array.value().capacity() as i64;

    Ok(Int::alloc(state.permanent_space.int_class(), cap))
}
