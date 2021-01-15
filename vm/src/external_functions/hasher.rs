//! Functions for hashing objects.
use crate::hasher::Hasher;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Creates a new hasher.
///
/// This function requires the following arguments:
///
/// 1. The first key to use to seed the hasher.
/// 2. The second key to use to seed the hasher.
pub fn hasher_new(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let key0 = arguments[0].u64_value()?;
    let key1 = arguments[1].u64_value()?;
    let hasher = Hasher::new(key0, key1);

    Ok(process.allocate(object_value::hasher(hasher), state.hasher_prototype))
}

/// Writes an object to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The object to write to the hasher.
pub fn hasher_write(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let hasher = arguments[0];

    arguments[1].hash_object(hasher.hasher_value_mut()?)?;
    Ok(hasher)
}

/// Gets a hash from a hasher.
///
/// This function requires a single argument: the hasher to get the hash from.
pub fn hasher_to_hash(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = arguments[0].hasher_value()?.to_hash();

    Ok(process.allocate_i64(result, state.integer_prototype))
}

register!(hasher_new, hasher_write, hasher_to_hash);
