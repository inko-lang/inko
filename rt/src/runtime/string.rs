use crate::mem::{PrimitiveString, PrimitiveStringResult};
use crate::result::Result as InkoResult;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;
use std::str;
use unicode_segmentation::{Graphemes, UnicodeSegmentation};

#[no_mangle]
pub unsafe extern "system" fn inko_string_is_valid_utf8(
    bytes: *const u8,
    size: i64,
) -> bool {
    let bytes = slice::from_raw_parts(bytes, size as usize);

    str::from_utf8(bytes).is_ok()
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_from_bytes(
    bytes: *const u8,
    size: i64,
) -> PrimitiveStringResult {
    let bytes = slice::from_raw_parts(bytes, size as usize);

    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(v) => PrimitiveStringResult::borrowed(v),
        Cow::Owned(v) => PrimitiveStringResult::owned(v),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_try_from_bytes(
    bytes: *const u8,
    size: i64,
) -> PrimitiveString {
    let bytes = slice::from_raw_parts(bytes, size as usize);

    str::from_utf8(bytes)
        .map(PrimitiveString::borrowed)
        .unwrap_or_else(|_| PrimitiveString::invalid())
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_lower(
    string: PrimitiveString,
) -> PrimitiveString {
    PrimitiveString::owned(string.as_str().to_lowercase())
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_upper(
    string: PrimitiveString,
) -> PrimitiveString {
    PrimitiveString::owned(string.as_str().to_uppercase())
}

#[no_mangle]
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
    let slice =
        str::from_utf8_unchecked(slice::from_raw_parts(bytes, size as _));

    let parsed = match slice {
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => slice.parse::<f64>(),
    };

    parsed
        .map(|v| InkoResult::ok(v.to_bits() as _))
        .unwrap_or_else(|_| InkoResult::none())
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_chars(
    string: PrimitiveString,
) -> *mut u8 {
    // Safety: a Graphemes takes a reference to a slice of bytes. The standard
    // library implements a wrapper around this native type that holds on to the
    // string we're iterating over, preventing the slice from being invalidated
    // while this iterator still exists.
    //
    // Graphemes isn't FFI safe, so we have to work around this by casting it to
    // a regular raw pointer.
    Box::into_raw(Box::new(string.as_str().graphemes(true))) as _
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_chars_next(
    iter: *mut u8,
) -> PrimitiveString {
    let iter = &mut *(iter as *mut Graphemes);

    iter.next()
        .map(PrimitiveString::borrowed)
        .unwrap_or_else(PrimitiveString::empty)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_chars_drop(iter: *mut u8) {
    drop(Box::from_raw(iter as *mut Graphemes));
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_from_pointer(
    ptr: *const c_char,
) -> PrimitiveString {
    PrimitiveString::owned(CStr::from_ptr(ptr).to_string_lossy().into_owned())
}
