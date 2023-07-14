use crate::mem::{Float, String as InkoString};
use crate::state::State;

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
