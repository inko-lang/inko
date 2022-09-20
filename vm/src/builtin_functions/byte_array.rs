use crate::immutable_string::ImmutableString;
use crate::mem::{ByteArray, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

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
