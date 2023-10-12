use crate::mem::String as InkoString;
use crate::state::State;

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
