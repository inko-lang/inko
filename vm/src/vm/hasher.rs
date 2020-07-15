//! VM functions for hashing objects.
use crate::hasher::Hasher;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn hasher_new(
    process: &RcProcess,
    key0_ptr: ObjectPointer,
    key1_ptr: ObjectPointer,
    proto_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let key0 = key0_ptr.u64_value()?;
    let key1 = key1_ptr.u64_value()?;
    let hasher = Hasher::new(key0, key1);

    Ok(process.allocate(object_value::hasher(hasher), proto_ptr))
}

#[inline(always)]
pub fn hasher_write(
    hasher: ObjectPointer,
    value: ObjectPointer,
) -> Result<ObjectPointer, String> {
    value.hash_object(hasher.hasher_value_mut()?)?;

    Ok(hasher)
}

#[inline(always)]
pub fn hasher_to_hash(
    state: &RcState,
    process: &RcProcess,
    hasher: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = hasher.hasher_value()?.to_hash();

    Ok(process.allocate_i64(result, state.integer_prototype))
}

#[inline(always)]
pub fn hasher_reset(hasher: ObjectPointer) -> Result<ObjectPointer, String> {
    hasher.hasher_value_mut()?.reset();

    Ok(hasher)
}
