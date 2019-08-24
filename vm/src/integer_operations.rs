//! Operations for manipulating integers that may overflow into big integers.

use num_bigint::BigInt;
use std::ops::{Shl, Shr};

use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;

/// Shifts an integer to the left or right.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$to_shift`: the pointer to the integer to shift.
/// * `$shift_with`: the pointer to the integer to shift with.
/// * `$proto`: the pointer to the prototype to use for allocating integers.
/// * `$op`: the operation to perform without checking for overflows.
/// * `$overflow_op`: the operation to perform while checking for overflows.
/// * `$opposite_op`: the opposite operator of `$op`.
macro_rules! shift_integer {
    (
        $process:expr,
        $to_shift:expr,
        $shift_with:expr,
        $proto:expr,
        $op:ident,
        $overflow_op:ident,
        $opposite_op:ident
    ) => {{
        let to_shift = $to_shift.integer_value()?;
        let shift_with = $shift_with.integer_value()?;
        let (res, overflowed) = to_shift.$overflow_op(shift_with as u32);

        let pointer = if overflowed {
            let to_shift_big = BigInt::from(to_shift);

            let bigint = if $shift_with.is_in_i32_range() {
                // A negative shift inverts the shift operation. For example,
                // `10 << -2` is the same as `10 >> 2`.
                if shift_with < 0 {
                    to_shift_big.$opposite_op(shift_with.abs() as usize)
                } else {
                    to_shift_big.$op(shift_with as usize)
                }
            } else {
                return Err(shift_error($to_shift, $shift_with)?);
            };

            $process.allocate(object_value::bigint(bigint), $proto)
        } else if ObjectPointer::integer_too_large(res) {
            $process.allocate(object_value::integer(res), $proto)
        } else {
            ObjectPointer::integer(res)
        };

        Ok(pointer)
    }};
}

/// Shifts a big integer to the left or right.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$to_shift`: the pointer to the integer to shift.
/// * `$shift_with`: the pointer to the integer to shift with.
/// * `$proto`: the pointer to the prototype to use for allocating integers.
/// * `$op`: the operation to perform without checking for overflows.
/// * `$opposite_op`: the opposite operation of `$op`
macro_rules! shift_big_integer {
    (
        $process:expr,
        $to_shift:expr,
        $shift_with:expr,
        $proto:expr,
        $op:ident,
        $opposite_op:ident
    ) => {{
        let to_shift = $to_shift.bigint_value()?.clone();
        let shift_with = $shift_with.integer_value()?;

        let res = if shift_with < 0 {
            to_shift.$opposite_op(shift_with.abs() as usize)
        } else {
            to_shift.$op(shift_with as usize)
        };

        Ok($process.allocate(object_value::bigint(res), $proto))
    }};
}

/// Inverts an argument for a shift and performs the shift.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$to_shift`: the pointer to the integer to shift.
/// * `$shift_with`: the pointer to the integer to shift with.
/// * `$proto`: the pointer to the prototype to use for allocating integers.
/// * `$function`: the function to use for shifting the value.
macro_rules! invert_shift {
    (
        $process:expr,
        $to_shift:expr,
        $shift_with:expr,
        $proto:expr,
        $function:ident
    ) => {{
        let shift_with =
            invert_integer_for_shift($process, $shift_with, $proto);

        $function($process, $to_shift, shift_with, $proto)
    }};
}

pub fn invert_integer_for_shift(
    process: &RcProcess,
    integer: ObjectPointer,
    prototype: ObjectPointer,
) -> ObjectPointer {
    let value = -integer.integer_value().unwrap();

    if ObjectPointer::integer_too_large(value) {
        process.allocate(object_value::integer(value), prototype)
    } else {
        ObjectPointer::integer(value)
    }
}

/// Returns true if the given integer should be inverted for a shift.
pub fn should_invert_for_shift(pointer: ObjectPointer) -> bool {
    pointer.is_integer() && pointer.integer_value().unwrap() < 0
}

/// Shifts an integer to the left. If the argument is negative it and the shift
/// operation are inverted.
pub fn integer_shift_left(
    process: &RcProcess,
    to_shift_ptr: ObjectPointer,
    shift_with_ptr: ObjectPointer,
    prototype: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if shift_with_ptr.is_in_u32_range() {
        // Example: 10 >> 5
        shift_integer!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            shl,
            overflowing_shl,
            shr
        )
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            integer_shift_right
        )
    } else {
        Err(shift_error(to_shift_ptr, shift_with_ptr)?)
    }
}

/// Shifts an integer to the right. If the argument is negative it and the shift
/// operation are inverted.
pub fn integer_shift_right(
    process: &RcProcess,
    to_shift_ptr: ObjectPointer,
    shift_with_ptr: ObjectPointer,
    prototype: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if shift_with_ptr.is_in_u32_range() {
        // Example: 10 >> 5
        shift_integer!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            shr,
            overflowing_shr,
            shl
        )
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            integer_shift_left
        )
    } else {
        Err(shift_error(to_shift_ptr, shift_with_ptr)?)
    }
}

/// Shifts a big integer to the left. If the argument is negative it and the
/// shift operation are inverted.
pub fn bigint_shift_left(
    process: &RcProcess,
    to_shift_ptr: ObjectPointer,
    shift_with_ptr: ObjectPointer,
    prototype: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if shift_with_ptr.is_in_i32_range() {
        shift_big_integer!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            shl,
            shr
        )
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            bigint_shift_right
        )
    } else {
        Err(shift_error(to_shift_ptr, shift_with_ptr)?)
    }
}

/// Shifts a big integer to the right. If the argument is negative it and the
/// shift operation are inverted.
pub fn bigint_shift_right(
    process: &RcProcess,
    to_shift_ptr: ObjectPointer,
    shift_with_ptr: ObjectPointer,
    prototype: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if shift_with_ptr.is_in_i32_range() {
        shift_big_integer!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            shr,
            shl
        )
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            bigint_shift_left
        )
    } else {
        Err(shift_error(to_shift_ptr, shift_with_ptr)?)
    }
}

/// Generates an error message to display in the event of a shift error.
fn shift_error(
    to_shift: ObjectPointer,
    shift_with: ObjectPointer,
) -> Result<String, String> {
    let message = if shift_with.is_integer() || shift_with.is_bigint() {
        format!(
            "Can't shift integer {} with {} as the operand is too big",
            to_shift.integer_to_string()?,
            shift_with.integer_to_string()?
        )
    } else {
        format!(
            "Can't shift integer {} because the operand is not an integer",
            to_shift.integer_to_string()?,
        )
    };

    Ok(message)
}
