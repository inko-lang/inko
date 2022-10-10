// VM functions for working with Inko integers.
use crate::mem::Pointer;
use crate::mem::{Float, Int, String as InkoString};
use crate::numeric::Modulo;
use crate::state::State;
use std::ops::{BitAnd, BitOr, BitXor};

macro_rules! overflow_error {
    ($left: expr, $right: expr) => {{
        return Err(format!(
            "Integer overflow, left: {}, right: {}",
            $left, $right
        ));
    }};
}

macro_rules! int_overflow_op {
    ($kind: ident, $left: expr, $right: expr, $op: ident) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        if let Some(result) = left.$op(right) {
            result
        } else {
            overflow_error!(left, right);
        }
    }};
}

macro_rules! int_op {
    ($left: expr, $right: expr, $op: ident) => {{
        let left = unsafe { Int::read($left) };
        let right = unsafe { Int::read($right) };

        left.$op(right)
    }};
}

macro_rules! int_shift {
    ($kind: ident, $left: expr, $right: expr, $op: ident) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        if let Some(result) = left.$op(right as u32) {
            result
        } else {
            overflow_error!(left, right);
        }
    }};
}

macro_rules! int_bool {
    ($kind: ident, $left: expr, $right: expr, $op: tt) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        if left $op right {
            Pointer::true_singleton()
        } else {
            Pointer::false_singleton()
        }
    }};
}

#[inline(always)]
pub(crate) fn add(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_overflow_op!(Int, left, right, checked_add);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn div(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    // This implements floored division, rather than rounding towards zero. This
    // makes division work more natural when using negative numbers.
    let lhs = unsafe { Int::read(left) };
    let rhs = unsafe { Int::read(right) };

    if rhs == 0 {
        return Err(format!(
            "Integer division failed, left: {}, right: {}",
            lhs, rhs
        ));
    }

    // Taken from the upcoming div_floor() implementation in the standard
    // library: https://github.com/rust-lang/rust/pull/88582.
    let d = lhs / rhs;
    let r = lhs % rhs;
    let value =
        if (r > 0 && rhs < 0) || (r < 0 && rhs > 0) { d - 1 } else { d };

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn mul(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_overflow_op!(Int, left, right, checked_mul);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn sub(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_overflow_op!(Int, left, right, checked_sub);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn modulo(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_overflow_op!(Int, left, right, checked_modulo);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn and(state: &State, left: Pointer, right: Pointer) -> Pointer {
    let value = int_op!(left, right, bitand);

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn or(state: &State, left: Pointer, right: Pointer) -> Pointer {
    let value = int_op!(left, right, bitor);

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn xor(state: &State, left: Pointer, right: Pointer) -> Pointer {
    let value = int_op!(left, right, bitxor);

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn not(state: &State, ptr: Pointer) -> Pointer {
    let value = unsafe { !Int::read(ptr) };

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn shl(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_shift!(Int, left, right, checked_shl);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn shr(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, String> {
    let value = int_shift!(Int, left, right, checked_shr);

    Ok(Int::alloc(state.permanent_space.int_class(), value))
}

#[inline(always)]
pub(crate) fn pow(state: &State, left: Pointer, right: Pointer) -> Pointer {
    let lhs = unsafe { Int::read(left) };
    let rhs = unsafe { Int::read(right) };
    let value = lhs.pow(rhs as u32);

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn lt(left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, left, right, <)
}

#[inline(always)]
pub(crate) fn gt(left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, left, right, >)
}

#[inline(always)]
pub(crate) fn eq(left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, left, right, ==)
}

#[inline(always)]
pub(crate) fn ge(left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, left, right, >=)
}

#[inline(always)]
pub(crate) fn le(left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, left, right, <=)
}

#[inline(always)]
pub(crate) fn clone(state: &State, ptr: Pointer) -> Pointer {
    if ptr.is_tagged_int() {
        return ptr;
    }

    let value = unsafe { Int::read(ptr) };

    Int::alloc(state.permanent_space.int_class(), value)
}

#[inline(always)]
pub(crate) fn to_float(state: &State, pointer: Pointer) -> Pointer {
    let value = unsafe { Int::read(pointer) } as f64;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn to_string(state: &State, pointer: Pointer) -> Pointer {
    let value = unsafe { Int::read(pointer) }.to_string();

    InkoString::alloc(state.permanent_space.string_class(), value)
}
