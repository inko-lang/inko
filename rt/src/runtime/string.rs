use crate::mem::{PrimitiveString, PrimitiveStringResult};
use crate::result::Result as InkoResult;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;
use std::str;
use unicode_segmentation::GraphemeCursor;

#[repr(C)]
pub struct Grapheme {
    pub start: i64,
    pub size: i64,
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_is_valid_utf8(
    bytes: *const u8,
    size: i64,
) -> bool {
    let bytes = unsafe { slice::from_raw_parts(bytes, size as usize) };

    str::from_utf8(bytes).is_ok()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_from_bytes(
    bytes: *const u8,
    size: i64,
) -> PrimitiveStringResult {
    let bytes = unsafe { slice::from_raw_parts(bytes, size as usize) };

    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(v) => PrimitiveStringResult::borrowed(v),
        Cow::Owned(v) => PrimitiveStringResult::owned(v),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_try_from_bytes(
    bytes: *const u8,
    size: i64,
) -> PrimitiveString {
    let bytes = unsafe { slice::from_raw_parts(bytes, size as usize) };

    str::from_utf8(bytes)
        .map(PrimitiveString::borrowed)
        .unwrap_or_else(|_| PrimitiveString::invalid())
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_to_lower(
    string: PrimitiveString,
) -> PrimitiveString {
    PrimitiveString::owned(string.as_str().to_lowercase())
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_to_upper(
    string: PrimitiveString,
) -> PrimitiveString {
    PrimitiveString::owned(string.as_str().to_uppercase())
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_to_float(
    bytes: *mut u8,
    size: i64,
) -> InkoResult {
    // On paper this relies on undefined behaviour, should the slice cover a
    // range bytes that's not valid UTF-8. But in reality this shouldn't pose a
    // problem because:
    //
    // 1. The float parser almost immediately converts the `&str` to `&[u8]`
    //    then operates on that, not caring about the encoding.
    // 2. Simply storing invalid UTF-8 in a `&str` isn't undefined behaviour
    //    (see https://github.com/rust-lang/rust/issues/71033), but using
    //    certain methods that expect it to be valid UTF-8 _may_ lead to
    //    undefined behaviour. Because of the previous item, this shouldn't be a
    //    problem.
    //
    // Long term we want to replace this function with a pure Inko
    // implementation, solving this problem entirely, but that proved to be too
    // much work at the time of writing this comment.
    let slice = unsafe {
        str::from_utf8_unchecked(slice::from_raw_parts(bytes, size as _))
    };

    let parsed = match slice {
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => slice.parse::<f64>(),
    };

    parsed
        .map(|v| InkoResult::ok(v.to_bits() as _))
        .unwrap_or_else(|_| InkoResult::none())
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_next_grapheme(
    string: PrimitiveString,
    start: i64,
) -> Grapheme {
    if start == string.size {
        return Grapheme { start: 0, size: 0 };
    }

    let end = GraphemeCursor::new(start as _, string.size as _, true)
        .next_boundary(string.as_str(), 0)
        .unwrap()
        .unwrap();

    Grapheme { start, size: end as i64 - start }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_from_pointer(
    ptr: *const c_char,
) -> PrimitiveString {
    PrimitiveString::owned(
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned(),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_scalar_is_alphabetic(
    value: i64,
) -> bool {
    // Safety: the standard library only passes in valid values.
    unsafe { char::from_u32_unchecked(value as u32) }.is_alphabetic()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_scalar_is_numeric(
    value: i64,
) -> bool {
    // Safety: the standard library only passes in valid values.
    unsafe { char::from_u32_unchecked(value as u32) }.is_numeric()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_scalar_is_whitespace(
    value: i64,
) -> bool {
    // Safety: the standard library only passes in valid values.
    unsafe { char::from_u32_unchecked(value as u32) }.is_whitespace()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_scalar_is_lower(value: i64) -> bool {
    // Safety: the standard library only passes in valid values.
    unsafe { char::from_u32_unchecked(value as u32) }.is_lowercase()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn inko_string_scalar_is_upper(value: i64) -> bool {
    // Safety: the standard library only passes in valid values.
    unsafe { char::from_u32_unchecked(value as u32) }.is_uppercase()
}
