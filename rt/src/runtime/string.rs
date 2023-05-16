use crate::mem::{
    tagged_int, Array, Bool, ByteArray, Float, Int, Nil, String as InkoString,
};
use crate::process::ProcessPointer;
use crate::result::Result as InkoResult;
use crate::runtime::process::panic;
use crate::state::State;
use std::cmp::min;
use std::slice;
use unicode_segmentation::{Graphemes, UnicodeSegmentation};

#[no_mangle]
pub unsafe extern "system" fn inko_string_new_permanent(
    state: *const State,
    bytes: *const u8,
    length: usize,
) -> *const InkoString {
    let bytes = slice::from_raw_parts(bytes, length).to_vec();
    let string = String::from_utf8_unchecked(bytes);

    InkoString::alloc_permanent((*state).string_class, string)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_equals(
    state: *const State,
    left: *const InkoString,
    right: *const InkoString,
) -> *const Bool {
    let state = &*state;

    if InkoString::read(left) == InkoString::read(right) {
        state.true_singleton
    } else {
        state.false_singleton
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_size(
    state: *const State,
    string: *const InkoString,
) -> *const Int {
    let state = &*state;

    Int::new(state.int_class, InkoString::read(string).len() as i64)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_concat(
    state: *const State,
    strings: *const *const InkoString,
    length: i64,
) -> *const InkoString {
    let slice = slice::from_raw_parts(strings, length as usize);
    let mut buffer = String::new();

    for &val in slice {
        buffer.push_str(InkoString::read(val));
    }

    InkoString::alloc((*state).string_class, buffer)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_byte(
    string: *const InkoString,
    index: i64,
) -> *const Int {
    let byte = i64::from(
        *InkoString::read(string).as_bytes().get_unchecked(index as usize),
    );

    tagged_int(byte)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_drop(
    state: *const State,
    pointer: *const InkoString,
) -> *const Nil {
    InkoString::drop(pointer);
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_lower(
    state: *const State,
    string: *const InkoString,
) -> *const InkoString {
    InkoString::alloc(
        (*state).string_class,
        InkoString::read(string).to_lowercase(),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_upper(
    state: *const State,
    string: *const InkoString,
) -> *const InkoString {
    InkoString::alloc(
        (*state).string_class,
        InkoString::read(string).to_uppercase(),
    )
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_byte_array(
    state: *const State,
    string: *const InkoString,
) -> *mut ByteArray {
    let bytes = InkoString::read(string).as_bytes().to_vec();

    ByteArray::alloc((*state).byte_array_class, bytes)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_float(
    state: *const State,
    string: *const InkoString,
    start: i64,
    end: i64,
) -> InkoResult {
    let string = InkoString::read(string);
    let slice = if start >= 0 && end >= 0 {
        &string[start as usize..end as usize]
    } else {
        string
    };

    let parsed = match slice {
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => slice.parse::<f64>(),
    };

    parsed
        .map(|v| InkoResult::ok(Float::alloc((*state).float_class, v) as _))
        .unwrap_or_else(|_| InkoResult::none())
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_to_int(
    state: *const State,
    process: ProcessPointer,
    string: *const InkoString,
    radix: i64,
    start: i64,
    end: i64,
) -> InkoResult {
    let string = InkoString::read(string);

    if !(2..=36).contains(&radix) {
        panic(process, &format!("The radix '{}' is invalid", radix));
    }

    let slice = if start >= 0 && end >= 0 {
        &string[start as usize..end as usize]
    } else {
        string
    };

    // Rust doesn't handle parsing strings like "-0x4a3f043013b2c4d1" out of the
    // box.
    let parsed = if radix == 16 {
        if let Some(tail) = string.strip_prefix("-0x") {
            i64::from_str_radix(tail, 16).map(|v| 0_i64.wrapping_sub(v))
        } else {
            i64::from_str_radix(slice, 16)
        }
    } else {
        i64::from_str_radix(slice, radix as u32)
    };

    parsed
        .map(|v| InkoResult::ok(Int::new((*state).int_class, v) as _))
        .unwrap_or_else(|_| InkoResult::none())
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_characters(
    string: *const InkoString,
) -> *mut u8 {
    let string = InkoString::read(string);

    // Safety: a Graphemes takes a reference to a slice of bytes. The standard
    // library implements a wrapper around this native type that holds on to the
    // string we're iterating over, preventing the slice from being invalidated
    // while this iterator still exists.
    //
    // Graphemes isn't FFI safe, so we have to work around this by casting it to
    // a regular raw pointer.
    Box::into_raw(Box::new(string.graphemes(true))) as _
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_characters_next(
    state: *const State,
    iter: *mut u8,
) -> InkoResult {
    let iter = &mut *(iter as *mut Graphemes);

    iter.next()
        .map(|v| {
            let string =
                InkoString::alloc((*state).string_class, v.to_string());

            InkoResult::ok(string as _)
        })
        .unwrap_or_else(InkoResult::none)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_characters_drop(
    state: *const State,
    iter: *mut u8,
) -> *const Nil {
    drop(Box::from_raw(iter as *mut Graphemes));
    (*state).nil_singleton
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_concat_array(
    state: *const State,
    array: *const Array,
) -> *const InkoString {
    let array = &*array;
    let mut buffer = String::new();

    for &ptr in &array.value {
        let ptr = ptr as *const InkoString;

        buffer.push_str(InkoString::read(ptr));
    }

    InkoString::alloc((*state).string_class, buffer)
}

#[no_mangle]
pub unsafe extern "system" fn inko_string_slice_bytes(
    state: *const State,
    string: *const InkoString,
    start: i64,
    length: i64,
) -> *const InkoString {
    let string = InkoString::read(string);
    let end = min((start + length) as usize, string.len());
    let new_string = if start < 0 || length <= 0 || start as usize >= end {
        String::new()
    } else {
        String::from_utf8_lossy(&string.as_bytes()[start as usize..end])
            .into_owned()
    };

    InkoString::alloc((*state).string_class, new_string)
}
