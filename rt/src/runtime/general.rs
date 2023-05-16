use crate::context;
use crate::mem::{free, header_of, is_tagged_int, ClassPointer};
use crate::process::ProcessPointer;
use crate::runtime::exit;
use crate::runtime::process::panic;
use std::alloc::alloc;

#[no_mangle]
pub unsafe extern "system" fn inko_exit(status: i64) {
    exit(status as i32);
}

#[no_mangle]
pub unsafe extern "system" fn inko_check_refs(
    process: ProcessPointer,
    pointer: *const u8,
) {
    if is_tagged_int(pointer) {
        return;
    }

    let header = header_of(pointer);

    if header.is_permanent() {
        return;
    }

    let refs = header.references();

    if refs == 0 {
        return;
    }

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
    if is_tagged_int(pointer) || header_of(pointer).is_permanent() {
        return;
    }

    free(pointer);
}

#[no_mangle]
pub unsafe extern "system" fn inko_reduce(
    mut process: ProcessPointer,
    amount: u16,
) {
    let thread = process.thread();

    thread.reductions = thread.reductions.saturating_sub(amount);

    if thread.reductions == 0 {
        // Safety: the current thread is holding on to the run lock
        thread.schedule(process);
        context::switch(process);
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_alloc(class: ClassPointer) -> *mut u8 {
    let ptr = alloc(class.instance_layout());

    header_of(ptr).init(class);
    ptr
}
