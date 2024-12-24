use crate::mem::{Type, TypePointer};
use std::{ffi::CStr, os::raw::c_char};

#[no_mangle]
pub unsafe extern "system" fn inko_type_object(
    name: *const c_char,
    size: u32,
    methods: u16,
) -> TypePointer {
    let name =
        String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).into_owned();

    Type::object(name, size, methods)
}

#[no_mangle]
pub unsafe extern "system" fn inko_type_process(
    name: *const c_char,
    size: u32,
    methods: u16,
) -> TypePointer {
    let name =
        String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).into_owned();

    Type::process(name, size, methods)
}
