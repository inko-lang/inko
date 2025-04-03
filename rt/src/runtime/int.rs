#[repr(C)]
pub struct CheckedIntResult {
    pub value: i64,
    pub tag: u8,
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_checked_pow(
    left: i64,
    right: i64,
) -> CheckedIntResult {
    if let Some(value) = left.checked_pow(right as u32) {
        CheckedIntResult { value, tag: 0 }
    } else {
        CheckedIntResult { value: 0, tag: 1 }
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_wrapping_pow(
    left: i64,
    right: i64,
) -> i64 {
    left.wrapping_pow(right as u32)
}
