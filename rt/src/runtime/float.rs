use crate::mem::PrimitiveString;

#[no_mangle]
pub unsafe extern "system" fn inko_float_to_string(
    value: f64,
) -> PrimitiveString {
    let string = if value.is_infinite() && value.is_sign_positive() {
        "Infinity".to_string()
    } else if value.is_infinite() {
        "-Infinity".to_string()
    } else if value.is_nan() {
        "NaN".to_string()
    } else {
        format!("{:?}", value)
    };

    PrimitiveString::owned(string)
}
