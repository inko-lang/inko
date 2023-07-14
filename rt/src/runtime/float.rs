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
