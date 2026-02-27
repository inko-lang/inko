use crate::mem::PrimitiveStringResult;

#[no_mangle]
pub unsafe extern "system" fn inko_float_to_string(
    value: f64,
) -> PrimitiveStringResult {
    if value.is_infinite() && value.is_sign_positive() {
        PrimitiveStringResult::borrowed("Infinity")
    } else if value.is_infinite() {
        PrimitiveStringResult::borrowed("-Infinity")
    } else if value.is_nan() {
        PrimitiveStringResult::borrowed("NaN")
    } else {
        PrimitiveStringResult::owned(format!("{:?}", value))
    }
}
