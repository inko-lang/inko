pub mod gc {
    mod baker;
    mod immix;
}

pub mod call_frame;
pub mod compiled_code;
pub mod constant_cache;
pub mod heap;
pub mod instruction;
pub mod memory_manager;
pub mod object;
pub mod register;
pub mod thread;
pub mod thread_list;
pub mod virtual_machine;
pub mod variable_scope;
