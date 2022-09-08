//! VM functions for working with Inko floats.
use crate::mem::Pointer;
use crate::mem::{Float, Int, String as InkoString};
use crate::state::State;
use float_cmp::ApproxEqUlps;

#[inline(always)]
pub(crate) fn add(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left + right;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn mul(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left * right;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn div(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left / right;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn sub(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left - right;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn modulo(state: &State, left: Pointer, right: Pointer) -> Pointer {
    let lhs = unsafe { Float::read(left) };
    let rhs = unsafe { Float::read(right) };
    let value = ((lhs % rhs) + rhs) % rhs;

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn eq(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if !left.is_nan() && !right.is_nan() && left.approx_eq_ulps(&right, 1) {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn lt(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left < right {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn gt(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left > right {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn ge(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left >= right {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn le(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left <= right {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn clone(state: &State, ptr: Pointer) -> Pointer {
    if ptr.is_permanent() {
        return ptr;
    }

    let value = unsafe { Float::read(ptr) };

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn ceil(state: &State, pointer: Pointer) -> Pointer {
    let float = unsafe { Float::read(pointer) };
    let value = float.ceil();

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn floor(state: &State, pointer: Pointer) -> Pointer {
    let float = unsafe { Float::read(pointer) };
    let value = float.floor();

    Float::alloc(state.permanent_space.float_class(), value)
}

#[inline(always)]
pub(crate) fn round(
    state: &State,
    pointer: Pointer,
    precision_ptr: Pointer,
) -> Pointer {
    let float = unsafe { Float::read(pointer) };
    let precision = unsafe { Int::read(precision_ptr) };
    let result = if precision == 0 {
        float.round()
    } else if precision <= i64::from(u32::MAX) {
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

    Float::alloc(state.permanent_space.float_class(), result)
}

#[inline(always)]
pub(crate) fn to_int(state: &State, pointer: Pointer) -> Pointer {
    let float = unsafe { Float::read(pointer) };

    Int::alloc(state.permanent_space.int_class(), float as i64)
}

#[inline(always)]
pub(crate) fn to_string(state: &State, pointer: Pointer) -> Pointer {
    let value = unsafe { Float::read(pointer) };
    let string = if value.is_infinite() && value.is_sign_positive() {
        "Infinity".to_string()
    } else if value.is_infinite() {
        "-Infinity".to_string()
    } else if value.is_nan() {
        "NaN".to_string()
    } else {
        format!("{:?}", value)
    };

    InkoString::alloc(state.permanent_space.string_class(), string)
}

#[inline(always)]
pub(crate) fn is_nan(pointer: Pointer) -> Pointer {
    if unsafe { Float::read(pointer) }.is_nan() {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn is_inf(pointer: Pointer) -> Pointer {
    if unsafe { Float::read(pointer) }.is_infinite() {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}
