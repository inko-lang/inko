use crate::mem::Int;
use crate::state::State;

#[repr(C)]
pub struct CheckedIntResult {
    pub value: i64,
    pub tag: u8,
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
