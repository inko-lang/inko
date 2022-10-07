use crate::mem::{Array, ByteArray, Float, Int, Pointer, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;
use std::cmp::min;
use unicode_segmentation::{Graphemes, UnicodeSegmentation};

pub(crate) fn string_to_lower(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let lower = unsafe { InkoString::read(&args[0]).to_lowercase() };
    let res = InkoString::alloc(state.permanent_space.string_class(), lower);

    Ok(res)
}

pub(crate) fn string_to_upper(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let upper = unsafe { InkoString::read(&args[0]).to_uppercase() };
    let res = InkoString::alloc(state.permanent_space.string_class(), upper);

    Ok(res)
}

pub(crate) fn string_to_byte_array(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { InkoString::read(&args[0]).as_bytes().to_vec() };
    let res = ByteArray::alloc(state.permanent_space.byte_array_class(), bytes);

    Ok(res)
}

pub(crate) fn string_to_float(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&args[0]) };
    let parsed = match string {
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        _ => string.parse::<f64>(),
    };

    let res = parsed
        .map(|val| Float::alloc(state.permanent_space.float_class(), val))
        .unwrap_or_else(|_| Pointer::undefined_singleton());

    Ok(res)
}

pub(crate) fn string_to_int(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&args[0]) };
    let radix = unsafe { Int::read(args[1]) };

    if !(2..=36).contains(&radix) {
        return Err(RuntimeError::Panic(format!(
            "The radix '{}' is invalid",
            radix
        )));
    }

    let res = i64::from_str_radix(string, radix as u32)
        .map(|val| Int::alloc(state.permanent_space.int_class(), val))
        .unwrap_or_else(|_| Pointer::undefined_singleton());

    Ok(res)
}

pub(crate) fn string_characters(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&args[0]) };
    let iter = Pointer::boxed(string.graphemes(true));

    Ok(iter)
}

pub(crate) fn string_characters_next(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let iter = unsafe { args[0].get_mut::<Graphemes>() };
    let class = state.permanent_space.string_class();
    let res = iter
        .next()
        .map(|v| InkoString::alloc(class, v.to_string()))
        .unwrap_or_else(Pointer::undefined_singleton);

    Ok(res)
}

pub(crate) fn string_characters_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { args[0].drop_boxed::<Graphemes>() };
    Ok(Pointer::nil_singleton())
}

pub(crate) fn string_concat_array(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { args[0].get::<Array>() }.value();
    let mut buffer = String::new();

    for str_ptr in array.iter() {
        buffer.push_str(unsafe { InkoString::read(str_ptr) });
    }

    Ok(InkoString::alloc(state.permanent_space.string_class(), buffer))
}

pub(crate) fn string_slice_bytes(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    args: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&args[0]) };
    let start = unsafe { Int::read(args[1]) };
    let len = unsafe { Int::read(args[2]) };
    let end = min((start + len) as usize, string.len());

    let new_string = if start < 0 || len <= 0 || start as usize >= end {
        String::new()
    } else {
        String::from_utf8_lossy(
            &string.as_bytes()[start as usize..end as usize],
        )
        .into_owned()
    };

    Ok(InkoString::alloc(state.permanent_space.string_class(), new_string))
}
