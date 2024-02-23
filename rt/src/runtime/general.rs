use crate::mem::{free, header_of, ClassPointer};
use crate::process::ProcessPointer;
use crate::runtime::exit;
use crate::runtime::process::panic;
use std::alloc::alloc;
use std::io::Error;

// Taken from Rust's standard library, with some removals of platforms we don't
// support.
extern "C" {
    #[cfg_attr(any(target_os = "linux",), link_name = "__errno_location")]
    #[cfg_attr(
        any(target_os = "netbsd", target_os = "android",),
        link_name = "__errno"
    )]
    #[cfg_attr(
        any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "freebsd",
            target_os = "watchos"
        ),
        link_name = "__error"
    )]
    fn errno_location() -> *mut i32;
}

#[no_mangle]
pub unsafe extern "system" fn inko_exit(status: i64) {
    exit(status as i32);
}

#[no_mangle]
pub unsafe extern "system" fn inko_reference_count_error(
    process: ProcessPointer,
    pointer: *const u8,
) {
    let header = header_of(pointer);
    let refs = header.references();

    panic(
        process,
        &format!(
            "can't drop a value of type '{}' as it still has {} reference(s)",
            &header.class.name, refs
        ),
    );
}

#[no_mangle]
pub unsafe extern "system" fn inko_free(pointer: *mut u8) {
    free(pointer);
}

#[no_mangle]
pub unsafe extern "system" fn inko_alloc(class: ClassPointer) -> *mut u8 {
    let ptr = alloc(class.instance_layout());

    header_of(ptr).init(class);
    ptr
}

#[no_mangle]
pub unsafe extern "system" fn inko_alloc_atomic(
    class: ClassPointer,
) -> *mut u8 {
    let ptr = alloc(class.instance_layout());

    header_of(ptr).init_atomic(class);
    ptr
}

#[no_mangle]
pub unsafe extern "system" fn inko_last_error() -> i32 {
    Error::last_os_error().raw_os_error().unwrap_or(-1)
}

#[no_mangle]
pub unsafe extern "system" fn inko_reset_error() {
    *errno_location() = 0;
}
