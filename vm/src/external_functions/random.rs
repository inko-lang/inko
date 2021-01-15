//! Functions for generating random numbers.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;
use num_bigint::RandBigInt;
use num_bigint::{BigInt, ToBigInt};
use rand::{thread_rng, Rng};
use std::cell::Cell;

macro_rules! verify_min_max {
    ($min:expr, $max:expr) => {
        if $min >= $max {
            return Err(format!(
                "The lower bound {} must be lower than the upper bound {}",
                $min, $max
            )
            .into());
        }
    };
}

thread_local! {
    /// A randomly generated, thread-local integer that can be incremented.
    ///
    /// This is useful when generating seed keys for hash maps. The first time
    /// this is used, a random number is generated. After that, the number is
    /// simply incremented (and wraps upon overflowing). This removes the need
    /// for generating a random number every time, which can be expensive.
    static INCREMENTAL_INTEGER: Cell<u64> = Cell::new(thread_rng().gen());
}

/// Generates a random integer.
///
/// This function doesn't take any arguments.
pub fn random_integer(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    Ok(process.allocate_i64(thread_rng().gen(), state.integer_prototype))
}

/// Generates an integer that starts of with a random value, then is
/// incremented on every call.
///
/// This function doesn't take any arguments.
pub fn random_incremental_integer(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let number =
        INCREMENTAL_INTEGER.with(|num| num.replace(num.get().wrapping_add(1)));

    Ok(process.allocate_u64(number, state.integer_prototype))
}

/// Generates a random float.
///
/// This function doesn't take any arguments.
pub fn random_float(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let value = object_value::float(thread_rng().gen());

    Ok(process.allocate(value, state.float_prototype))
}

/// Generates a random integer in a range.
///
/// This function takes two arguments:
///
/// 1. The lower bound of the range.
/// 2. The upper bound of the range.
pub fn random_integer_range(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let min_ptr = arguments[0];
    let max_ptr = arguments[1];
    let mut rng = thread_rng();

    if min_ptr.is_integer() && max_ptr.is_integer() {
        let min = min_ptr.integer_value()?;
        let max = max_ptr.integer_value()?;

        verify_min_max!(min, max);

        Ok(process
            .allocate_i64(rng.gen_range(min, max), state.integer_prototype))
    } else if min_ptr.is_bigint() && max_ptr.is_bigint() {
        let min = to_bigint(min_ptr)?;
        let max = to_bigint(max_ptr)?;

        verify_min_max!(min, max);

        Ok(process.allocate(
            object_value::bigint(rng.gen_bigint_range(&min, &max)),
            state.integer_prototype,
        ))
    } else {
        Err(RuntimeError::from(
            "random_integer_range only supports integers for the range bounds",
        ))
    }
}

/// Generates a random float in a range.
///
/// This function takes two arguments:
///
/// 1. The lower bound of the range.
/// 2. The upper bound of the range.
pub fn random_float_range(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let min = arguments[0].float_value()?;
    let max = arguments[1].float_value()?;
    let mut rng = thread_rng();

    verify_min_max!(min, max);

    Ok(process.allocate(
        object_value::float(rng.gen_range(min, max)),
        state.float_prototype,
    ))
}

/// Generates a random sequence of bytes.
///
/// This function takes a single argument: the number of bytes to generate.
pub fn random_bytes(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let size = arguments[0].usize_value()?;
    let mut bytes = Vec::with_capacity(size);

    unsafe {
        bytes.set_len(size);
    }

    thread_rng()
        .try_fill(&mut bytes[..])
        .map_err(|e| e.to_string())?;

    Ok(process
        .allocate(object_value::byte_array(bytes), state.byte_array_prototype))
}

fn to_bigint(pointer: ObjectPointer) -> Result<BigInt, String> {
    if let Ok(bigint) = pointer.bigint_value() {
        Ok(bigint.clone())
    } else {
        Ok(pointer.integer_value()?.to_bigint().unwrap())
    }
}

register!(
    random_integer,
    random_incremental_integer,
    random_float,
    random_integer_range,
    random_float_range,
    random_bytes
);
