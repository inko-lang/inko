//! VM functions for working with Inko floats.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;
use float_cmp::ApproxEqUlps;
use std::i32;
use std::i64;

#[inline(always)]
pub fn float_to_integer(
    state: &RcState,
    process: &RcProcess,
    float: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float_val = float.float_value()?;

    process.allocate_f64_as_i64(float_val, state.integer_prototype)
}

#[inline(always)]
pub fn float_to_string(
    state: &RcState,
    process: &RcProcess,
    float: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = format!("{:?}", float.float_value()?);

    Ok(process.allocate(object_value::string(result), state.string_prototype))
}

#[inline(always)]
pub fn float_equals(
    state: &RcState,
    compare_ptr: ObjectPointer,
    compare_with_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let compare = compare_ptr.float_value()?;
    let compare_with = compare_with_ptr.float_value()?;

    let boolean = if !compare.is_nan()
        && !compare_with.is_nan()
        && compare.approx_eq_ulps(&compare_with, 2)
    {
        state.true_object
    } else {
        state.false_object
    };

    Ok(boolean)
}

#[inline(always)]
pub fn float_is_nan(state: &RcState, pointer: ObjectPointer) -> ObjectPointer {
    let is_nan = match pointer.float_value() {
        Ok(float) => float.is_nan(),
        Err(_) => false,
    };

    if is_nan {
        state.true_object
    } else {
        state.false_object
    }
}

#[inline(always)]
pub fn float_is_infinite(
    state: &RcState,
    pointer: ObjectPointer,
) -> ObjectPointer {
    let is_inf = match pointer.float_value() {
        Ok(float) => float.is_infinite(),
        Err(_) => false,
    };

    if is_inf {
        state.true_object
    } else {
        state.false_object
    }
}

#[inline(always)]
pub fn float_floor(
    state: &RcState,
    process: &RcProcess,
    pointer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float = pointer.float_value()?.floor();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

#[inline(always)]
pub fn float_ceil(
    state: &RcState,
    process: &RcProcess,
    pointer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float = pointer.float_value()?.ceil();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

#[inline(always)]
pub fn float_round(
    state: &RcState,
    process: &RcProcess,
    pointer: ObjectPointer,
    prec_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let precision = prec_ptr.integer_value()?;
    let float = pointer.float_value()?;

    let result = if precision == 0 {
        float.round()
    } else if precision >= i64::from(i32::MIN)
        && precision <= i64::from(i32::MAX)
    {
        let power = 10.0_f64.powi(precision as i32);
        let multiplied = float * power;

        // Certain very large numbers (e.g. f64::MAX) would
        // produce Infinity when multiplied with the power. In
        // this case we just return the input float directly.
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

#[inline(always)]
pub fn float_to_bits(
    state: &RcState,
    process: &RcProcess,
    float_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let bits = float_ptr.float_value()?.to_bits();

    Ok(process.allocate_u64(bits, state.integer_prototype))
}
