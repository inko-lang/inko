use crate::mem::PrimitiveString;

#[no_mangle]
pub unsafe extern "system" fn inko_float_to_string(
    value: f64,
) -> PrimitiveString {
    let mut string = if value.is_infinite() && value.is_sign_positive() {
        "Infinity\0".to_string()
    } else if value.is_infinite() {
        "-Infinity\0".to_string()
    } else if value.is_nan() {
        "NaN\0".to_string()
    } else {
        format!("{:?}\0", value)
    };

    // We may over-allocate when formatting a float, so this ensures we don't
    // carry around extra unnecessary memory.
    string.shrink_to_fit();
    PrimitiveString::owned(string)
}
