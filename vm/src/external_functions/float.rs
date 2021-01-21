//! Functions for working with Inko floats.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Rounds a float up.
///
/// This function requires a single argument: the float to round.
pub fn float_ceil(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let float = arguments[0].float_value()?.ceil();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

/// Rounds a float down.
///
/// This function requires a single argument: the float to round.
pub fn float_floor(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let float = arguments[0].float_value()?.floor();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

/// Rounds a float to a given number of decimals.
///
/// This function requires the following arguments:
///
/// 1. The float to round.
/// 2. The number of decimals to round to.
pub fn float_round(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let float = arguments[0].float_value()?;
    let precision = arguments[1].integer_value()?;

    let result = if precision == 0 {
        float.round()
    } else if precision >= i64::from(i32::MIN)
        && precision <= i64::from(i32::MAX)
    {
        let power = 10.0_f64.powi(precision as i32);
        let multiplied = float * power;

        // Certain very large numbers (e.g. f64::MAX) would produce Infinity
        // when multiplied with the power. In this case we just return the input
        // float directly.
        if multiplied.is_finite() {
            multiplied.round() / power
        } else {
            float
        }
    } else {
        float
    };

    Ok(process.allocate(object_value::float(result), state.float_prototype))
}

/// Returns the bitwise representation of the float.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_bits(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let bits = arguments[0].float_value()?.to_bits();

    Ok(process.allocate_u64(bits, state.integer_prototype))
}

/// Converts a Float to an Integer.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_integer(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let float_val = arguments[0].float_value()?;

    Ok(process.allocate_f64_as_i64(float_val, state.integer_prototype)?)
}

/// Converts a Float to a String.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_string(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = format!("{:?}", arguments[0].float_value()?);

    Ok(process.allocate(object_value::string(result), state.string_prototype))
}

/// Returns true if a Float is a NaN.
///
/// This function requires a single argument: the float to check.
pub fn float_is_nan(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = if arguments[0].float_value()?.is_nan() {
        state.true_object
    } else {
        state.false_object
    };

    Ok(result)
}

/// Returns true if a Float is an infinite number.
///
/// This function requires a single argument: the float to check.
pub fn float_is_infinite(
    state: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let result = if arguments[0].float_value()?.is_infinite() {
        state.true_object
    } else {
        state.false_object
    };

    Ok(result)
}

register!(
    float_ceil,
    float_floor,
    float_round,
    float_to_bits,
    float_to_integer,
    float_to_string,
    float_is_nan,
    float_is_infinite
);
