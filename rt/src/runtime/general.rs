use crate::mem::{header_of, ClassPointer};
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use std::alloc::handle_alloc_error;
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
pub unsafe extern "system" fn inko_reference_count_error(
    process: ProcessPointer,
    pointer: *const u8,
) {
    let header = header_of(pointer);

    panic(
        process,
        &format!(
            "can't drop a value of type '{}' as it still has {} reference(s)",
            &header.class.name, header.references
        ),
    );
}

#[no_mangle]
pub unsafe extern "system" fn inko_alloc_error(class: ClassPointer) -> ! {
    handle_alloc_error(class.instance_layout());
}

#[no_mangle]
pub unsafe extern "system" fn inko_last_error() -> i64 {
    Error::last_os_error().raw_os_error().unwrap_or(-1) as i64
}

#[no_mangle]
pub unsafe extern "system" fn inko_reset_error() {
    *errno_location() = 0;
}
