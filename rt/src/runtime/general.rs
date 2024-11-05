use crate::bump::free;
use crate::mem::header_of;
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use crate::runtime::process::PANIC_STATUS;
use std::process::exit;
use std::ptr;

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
pub unsafe extern "system" fn inko_alloc_error(size: u64) -> ! {
    // When running out of memory, chances are we don't have enough to produce a
    // stack trace or fancy error messages, so we try to keep things as
    // basic/simple as possible.
    eprintln!("failed to allocate {} bytes", size);
    exit(PANIC_STATUS);
}

#[no_mangle]
pub unsafe extern "system" fn inko_bump_allocate(
    mut process: ProcessPointer,
    size: i64,
) -> *mut u8 {
    let thread = process.thread();

    match size {
        16 => thread.bump_allocators[0].allocate(),
        24 => thread.bump_allocators[1].allocate(),
        32 => thread.bump_allocators[1].allocate(),
        40 => thread.bump_allocators[2].allocate(),
        48 => thread.bump_allocators[2].allocate(),
        56 => thread.bump_allocators[2].allocate(),
        64 => thread.bump_allocators[2].allocate(),
        128 => thread.bump_allocators[3].allocate(),
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_bump_free(pointer: *mut u8) {
    free(pointer);
}
