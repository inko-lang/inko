#![macro_use]

//! Matches the option and returns the wrapped value if present, exits with a VM
//! error when it's not.
//!
//! Example:
//!
//!     let option     = Option::None;
//!     let thread_ref = thread.borrow_mut();
//!
//!     let value = some_or_terminate!(option, self, thread_ref, "Bummer!");
//!
macro_rules! some_or_terminate {
    ($value: expr, $vm: expr, $thread: expr, $message: expr) => {
        match $value {
            Option::Some(wrapped) => {
                wrapped
            },
            Option::None => {
                $vm.terminate_vm(&$thread, $message);
            }
        }
    }
}
