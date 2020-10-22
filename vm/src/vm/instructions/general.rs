//! VM functions for which no better category/module exists.
use crate::execution_context::ExecutionContext;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn set_literal(context: &ExecutionContext, index: u16) -> ObjectPointer {
    unsafe { context.module.literal(index as usize) }
}

#[inline(always)]
pub fn set_literal_wide(
    context: &ExecutionContext,
    arg1: u16,
    arg2: u16,
) -> ObjectPointer {
    let index = (u32::from(arg1) << 16) | (u32::from(arg2) & 0xFFFF);

    unsafe { context.module.literal(index as usize) }
}

#[inline(always)]
pub fn set_local(
    context: &mut ExecutionContext,
    index: u16,
    value: ObjectPointer,
) {
    context.set_local(index, value);
}

#[inline(always)]
pub fn get_local(context: &mut ExecutionContext, index: u16) -> ObjectPointer {
    context.get_local(index)
}

#[inline(always)]
pub fn local_exists(
    state: &RcState,
    context: &ExecutionContext,
    local: u16,
) -> ObjectPointer {
    if context.binding.local_exists(local) {
        state.true_object
    } else {
        state.false_object
    }
}

#[inline(always)]
pub fn set_parent_local(
    context: &mut ExecutionContext,
    local: u16,
    depth: u16,
    value: ObjectPointer,
) -> Result<(), String> {
    if let Some(binding) = context.binding.find_parent(depth as usize) {
        binding.set_local(local, value);

        Ok(())
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

#[inline(always)]
pub fn get_parent_local(
    context: &ExecutionContext,
    local: u16,
    depth: u16,
) -> Result<ObjectPointer, String> {
    if let Some(binding) = context.binding.find_parent(depth as usize) {
        Ok(binding.get_local(local))
    } else {
        Err(format!("No binding for depth {}", depth))
    }
}

#[inline(always)]
pub fn set_global(
    state: &RcState,
    context: &mut ExecutionContext,
    index: u16,
    object: ObjectPointer,
) -> ObjectPointer {
    let value = if object.is_permanent() {
        object
    } else {
        state.permanent_allocator.lock().copy_object(object)
    };

    context.set_global(index, value);
    value
}

#[inline(always)]
pub fn get_global(context: &ExecutionContext, index: u16) -> ObjectPointer {
    context.get_global(index)
}

#[inline(always)]
pub fn exit(state: &RcState, status_ptr: ObjectPointer) -> Result<(), String> {
    let status = status_ptr.i32_value()?;

    state.terminate(status);
    Ok(())
}

#[inline(always)]
pub fn set_default_panic_handler(
    state: &RcState,
    handler: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if handler.block_value()?.captures_from.is_some() {
        return Err(
            "Default panic handlers can't capture any variables".to_string()
        );
    }

    let handler_to_use = if handler.is_permanent() {
        handler
    } else {
        state.permanent_allocator.lock().copy_object(handler)
    };

    state
        .default_panic_handler
        .raw
        .atomic_store(handler_to_use.raw.raw);

    Ok(handler_to_use)
}

#[inline(always)]
pub fn move_result(process: &RcProcess) -> ObjectPointer {
    process.take_result()
}
