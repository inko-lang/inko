// FIXME: re-enable once all code is actually used.
#![allow(dead_code)]

pub mod gc {
    mod baker;
    mod immix;
}

pub mod call_frame;
pub mod class;
pub mod compiled_code;
pub mod constant_cache;
pub mod heap;
pub mod instruction;
pub mod object;
pub mod register;
pub mod thread;
pub mod virtual_machine;
pub mod variable_scope;
