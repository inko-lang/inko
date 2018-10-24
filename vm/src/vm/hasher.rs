//! VM functions for hashing objects.
use hasher::Hasher;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::state::RcState;

pub fn create(state: &RcState, process: &RcProcess) -> ObjectPointer {
    process
        .allocate(object_value::hasher(Hasher::new()), state.hasher_prototype)
}

pub fn write(
    state: &RcState,
    hasher: ObjectPointer,
    value: ObjectPointer,
) -> Result<ObjectPointer, String> {
    value.hash_object(hasher.hasher_value_mut()?)?;

    Ok(state.nil_object)
}

pub fn finish(
    state: &RcState,
    process: &RcProcess,
    hasher: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = hasher.hasher_value_mut()?.finish();

    Ok(process.allocate_i64(result, state.integer_prototype))
}
