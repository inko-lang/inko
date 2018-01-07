//! Operations for manipulating integers that may overflow into big integers.

use std::ops::{Shl, Shr};
use num_bigint::BigInt;

use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;

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
macro_rules! shift_integer {
    (
        $process: expr,
        $to_shift: expr,
        $shift_with: expr,
        $proto: expr,
        $op: ident,
        $overflow_op: ident
    ) => ({
        let to_shift = $to_shift.integer_value()?;
        let shift_with = $shift_with.integer_value()?;
        let (res, overflowed) = to_shift.$overflow_op(shift_with as u32);

        let pointer = if overflowed {
            let bigint = BigInt::from(to_shift).$op(shift_with as usize);

            $process.allocate(object_value::bigint(bigint), $proto)
        } else if ObjectPointer::integer_too_large(res) {
            $process.allocate(object_value::integer(res), $proto)
        } else {
            ObjectPointer::integer(res)
        };

        Ok(pointer)
    });
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
macro_rules! shift_big_integer {
    (
        $process: expr,
        $to_shift: expr,
        $shift_with: expr,
        $proto: expr,
        $op: ident
    ) => ({
        let to_shift = $to_shift.bigint_value()?;
        let shift_with = $shift_with.integer_value()? as usize;
        let res = to_shift.clone().$op(shift_with);

        Ok($process.allocate(object_value::bigint(res), $proto))
    });
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
        $process: expr,
        $to_shift: expr,
        $shift_with: expr,
        $proto: expr,
        $function: ident
    ) => ({
        let shift_with =
            invert_integer_for_shift($process, $shift_with, $proto);

        $function($process, $to_shift, shift_with, $proto)
    });
}

/// Converts an integer or a bigint to a String.
macro_rules! format_integer {
    ($pointer: expr) => ({
        if $pointer.is_bigint() {
            $pointer.bigint_value()?.to_string()
        } else {
            $pointer.integer_value()?.to_string()
        }
    })
}

/// Generates an error message to display in the event of a shift error.
macro_rules! shift_error {
    ($to_shift: expr, $shift_with: expr) => ({
        if $shift_with.is_integer() || $shift_with.is_bigint() {
            format!(
                "Can't shift integer {} with {} as the value is too big",
                format_integer!($to_shift),
                format_integer!($shift_with)
            )
        } else {
            format!(
                "Can't shift integer {} because the operand is not an integer",
                format_integer!($to_shift),
            )
        }
    });
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
            overflowing_shl
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
        Err(shift_error!(to_shift_ptr, shift_with_ptr))
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
            overflowing_shr
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
        Err(shift_error!(to_shift_ptr, shift_with_ptr))
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
    if shift_with_ptr.is_in_u32_range() {
        shift_big_integer!(process, to_shift_ptr, shift_with_ptr, prototype, shl)
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            bigint_shift_right
        )
    } else {
        Err(shift_error!(to_shift_ptr, shift_with_ptr))
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
    if shift_with_ptr.is_in_u32_range() {
        shift_big_integer!(process, to_shift_ptr, shift_with_ptr, prototype, shr)
    } else if should_invert_for_shift(shift_with_ptr) {
        invert_shift!(
            process,
            to_shift_ptr,
            shift_with_ptr,
            prototype,
            bigint_shift_left
        )
    } else {
        Err(shift_error!(to_shift_ptr, shift_with_ptr))
    }
}
