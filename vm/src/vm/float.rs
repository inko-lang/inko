//! VM functions for working with Inko floats.
use float_cmp::ApproxEqUlps;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use std::i32;
use std::i64;
use vm::state::RcState;

pub fn to_integer(
    state: &RcState,
    process: &RcProcess,
    float: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float_val = float.float_value()?;

    process.allocate_f64_as_i64(float_val, state.integer_prototype)
}

pub fn to_string(
    state: &RcState,
    process: &RcProcess,
    float: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let result = format!("{:?}", float.float_value()?);

    Ok(process.allocate(object_value::string(result), state.string_prototype))
}

pub fn equal(
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

pub fn is_nan(state: &RcState, pointer: ObjectPointer) -> ObjectPointer {
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

pub fn is_infinite(state: &RcState, pointer: ObjectPointer) -> ObjectPointer {
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

pub fn floor(
    state: &RcState,
    process: &RcProcess,
    pointer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float = pointer.float_value()?.floor();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

pub fn ceil(
    state: &RcState,
    process: &RcProcess,
    pointer: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let float = pointer.float_value()?.ceil();

    Ok(process.allocate(object_value::float(float), state.float_prototype))
}

pub fn round(
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
