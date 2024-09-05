use crate::mem::{header_of, ClassPointer};
use crate::process::ProcessPointer;
use crate::runtime::process::panic;
use std::alloc::handle_alloc_error;

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
