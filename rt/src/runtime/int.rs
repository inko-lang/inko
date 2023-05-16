use crate::mem::{Float, Int, String as InkoString};
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use crate::state::State;

#[no_mangle]
pub unsafe extern "system" fn inko_int_overflow(
    process: ProcessPointer,
    left: i64,
    right: i64,
) -> ! {
    let message = format!("Int overflowed, left: {}, right: {}", left, right);

    panic(process, &message);
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_boxed(
    state: *const State,
    value: i64,
) -> *const Int {
    Int::boxed((*state).int_class, value)
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_boxed_permanent(
    state: *const State,
    value: i64,
) -> *const Int {
    Int::boxed_permanent((*state).int_class, value)
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_pow(
    process: ProcessPointer,
    left: i64,
    right: i64,
) -> i64 {
    if let Some(val) = left.checked_pow(right as u32) {
        val
    } else {
        inko_int_overflow(process, left, right);
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_clone(
    state: *const State,
    int: *const Int,
) -> *const Int {
    let obj = &*int;

    if obj.header.is_permanent() {
        int
    } else {
        Int::boxed((*state).int_class, obj.value)
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_to_float(
    state: *const State,
    int: i64,
) -> *const Float {
    Float::alloc((*state).float_class, int as f64)
}

#[no_mangle]
pub unsafe extern "system" fn inko_int_to_string(
    state: *const State,
    int: i64,
) -> *const InkoString {
    InkoString::alloc((*state).string_class, int.to_string())
}
