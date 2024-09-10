//! Context switching between thread/process stacks
//!
//! Context switching is implemented using two functions: `start` and `switch`.
//!
//! The `start` function starts a new message by calling the appropriate native
//! function. The `switch` function is used for switching from a thread to a
//! process and back. This function shouldn't be used to switch between two
//! processes directly.
//!
//! The `start` function passes the necessary arguments using a `Context` type.
//! This type is dropped after the first yield, so the native code must load its
//! components into variables/registers in order to continue using them.
use crate::process::{NativeAsyncMethod, ProcessPointer};
mod unix;

// These functions are defined in the inline assembly macros, found in modules
// such as context/unix/x86_64.rs.
extern "system" {
    fn inko_context_init(
        high: *mut *mut u8,
        func: NativeAsyncMethod,
        data: *mut u8,
    );

    fn inko_context_switch(stack: *mut *mut u8);
}

#[inline(always)]
pub(crate) unsafe fn start(
    mut process: ProcessPointer,
    func: NativeAsyncMethod,
    data: *mut u8,
) {
    inko_context_init(&mut process.stack_pointer, func as _, data);
}

#[inline(always)]
pub(crate) unsafe fn switch(mut process: ProcessPointer) {
    inko_context_switch(&mut process.stack_pointer);
}
