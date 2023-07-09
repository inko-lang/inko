use crate::mem::{Class, ClassPointer};
use std::{ffi::CStr, os::raw::c_char};

#[no_mangle]
pub unsafe extern "system" fn inko_class_object(
    name: *const c_char,
    size: u32,
    methods: u16,
) -> ClassPointer {
    let name =
        String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).into_owned();

    Class::object(name, size, methods)
}

#[no_mangle]
pub unsafe extern "system" fn inko_class_process(
    name: *const c_char,
    size: u32,
    methods: u16,
) -> ClassPointer {
    let name =
        String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).into_owned();

    Class::process(name, size, methods)
}

#[no_mangle]
pub unsafe extern "system" fn inko_class_drop(class: ClassPointer) {
    Class::drop(class);
}
