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
use crate::state::State;

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

/// A type storing state used when first starting a process.
///
/// Using this type allows us to keep the assembly used for setting up a process
/// simple, as we only need to pass a small number of arguments (which all fit
/// in registers).
///
/// This type is dropped after the first yield from a process back to its
/// thread.
#[repr(C)]
pub struct Context {
    pub state: *const State,
    pub process: ProcessPointer,
    pub arguments: *mut u8,
}

// These functions are defined in the inline assembly macros, found in modules
// such as context/unix/x86_64.rs.
extern "system" {
    #[cfg(windows)]
    fn inko_context_init(
        low: *mut u8,
        high: *mut *mut u8,
        func: NativeAsyncMethod,
        ctx: *mut u8,
    );

    #[cfg(not(windows))]
    fn inko_context_init(
        high: *mut *mut u8,
        func: NativeAsyncMethod,
        ctx: *mut u8,
    );

    fn inko_context_switch(stack: *mut *mut u8);
}

#[inline(never)]
#[cfg(not(windows))]
pub(crate) unsafe fn start(
    state: &State,
    mut process: ProcessPointer,
    func: NativeAsyncMethod,
    mut args: Vec<*mut u8>,
) {
    let ctx = Context {
        state: state as _,
        process,
        arguments: args.as_mut_ptr() as _,
    };

    inko_context_init(
        &mut process.stack_pointer,
        func as _,
        &ctx as *const _ as _,
    );
}

#[inline(never)]
#[cfg(windows)]
pub(crate) unsafe fn start(
    state: &State,
    mut process: ProcessPointer,
    func: NativeAsyncMethod,
    args: Vec<*mut u8>,
) {
    let ctx =
        Context { state: state as _, process, arguments: args.as_mut_ptr() };
    let low = process.stack.as_ref().unwrap().ptr();
    let high = &mut process.stack_pointer;

    inko_context_init(low, high, func as _, &ctx as *const _ as _);
}

#[inline(never)]
pub(crate) unsafe fn switch(mut process: ProcessPointer) {
    inko_context_switch(&mut process.stack_pointer);
}
