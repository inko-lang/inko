use crate::immutable_string::ImmutableString;
use crate::mem::{ByteArray, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;
use std::cmp::min;

pub(crate) fn byte_array_to_string(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes_ref = unsafe { args[0].get_mut::<ByteArray>() };
    let string = ImmutableString::from_utf8(bytes_ref.value().clone());
    let res = InkoString::from_immutable_string(
        state.permanent_space.string_class(),
        string,
    );

    Ok(res)
}

pub(crate) fn byte_array_drain_to_string(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes_ref = unsafe { args[0].get_mut::<ByteArray>() };
    let string = ImmutableString::from_utf8(bytes_ref.take_bytes());
    let res = InkoString::from_immutable_string(
        state.permanent_space.string_class(),
        string,
    );

    Ok(res)
}

pub(crate) fn byte_array_slice(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { args[0].get::<ByteArray>() };
    let start = unsafe { Int::read(args[1]) } as usize;
    let len = unsafe { Int::read(args[2]) } as usize;
    let end = min((start + len) as usize, bytes.value().len());

    Ok(ByteArray::alloc(
        state.permanent_space.byte_array_class(),
        bytes.value()[start..end].to_vec(),
    ))
}
