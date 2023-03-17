use crate::mem::{Bool, Float, String as InkoString};
use crate::state::State;

/// The maximum difference between two floats for them to be considered equal,
/// as expressed in "Units in the Last Place" (ULP).
const ULP_DIFF: i64 = 1;

#[no_mangle]
pub unsafe extern "system" fn inko_float_boxed(
    state: *const State,
    value: f64,
) -> *const Float {
    Float::alloc((*state).float_class, value)
}

#[no_mangle]
pub unsafe extern "system" fn inko_float_boxed_permanent(
    state: *const State,
    value: f64,
) -> *const Float {
    Float::alloc_permanent((*state).float_class, value)
}

#[no_mangle]
pub unsafe extern "system" fn inko_float_eq(
    state: *const State,
    left: f64,
    right: f64,
) -> *const Bool {
    // For float equality we use ULPs. See
    // https://randomascii.wordpress.com/2012/02/25/comparing-floating-point-numbers-2012-edition/
    // for more details.
    let state = &*state;

    if left == right {
        // Handle cases such as `-0.0 == 0.0`.
        return state.true_singleton;
    }

    if left.is_sign_positive() != right.is_sign_positive() {
        return state.false_singleton;
    }

    if left.is_nan() || right.is_nan() {
        return state.false_singleton;
    }

    let left_bits = left.to_bits() as i64;
    let right_bits = right.to_bits() as i64;
    let diff = left_bits.wrapping_sub(right_bits);

    if (-ULP_DIFF..=ULP_DIFF).contains(&diff) {
        state.true_singleton
    } else {
        state.false_singleton
    }
}

// TODO: do in LLVM?
#[no_mangle]
pub unsafe extern "system" fn inko_float_clone(
    state: *const State,
    float: *const Float,
) -> *const Float {
    let obj = &*float;

    if obj.header.is_permanent() {
        float
    } else {
        Float::alloc((*state).float_class, obj.value)
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_float_round(
    state: *const State,
    float: f64,
    precision: i64,
) -> *const Float {
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

    Float::alloc((*state).float_class, result)
}

#[no_mangle]
pub unsafe extern "system" fn inko_float_to_string(
    state: *const State,
    value: f64,
) -> *const InkoString {
    let string = if value.is_infinite() && value.is_sign_positive() {
        "Infinity".to_string()
    } else if value.is_infinite() {
        "-Infinity".to_string()
    } else if value.is_nan() {
        "NaN".to_string()
    } else {
        format!("{:?}", value)
    };

    InkoString::alloc((*state).string_class, string)
}
