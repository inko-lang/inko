//! VM functions for working with Inko blocks.
use crate::block::Block;
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn block_get_receiver(context: &ExecutionContext) -> ObjectPointer {
    *context.binding.receiver()
}

#[inline(always)]
pub fn run_block(
    process: &RcProcess,
    context: &ExecutionContext,
    block_ptr: ObjectPointer,
    start_reg: u16,
    amount: u16,
) -> Result<(), String> {
    let block = block_ptr.block_value()?;
    let mut new_context = ExecutionContext::from_block(&block);

    prepare_block_arguments!(context, new_context, start_reg, amount);
    process.push_context(new_context);

    Ok(())
}

#[inline(always)]
pub fn run_block_with_receiver(
    process: &RcProcess,
    context: &ExecutionContext,
    block_ptr: ObjectPointer,
    receiver_ptr: ObjectPointer,
    start_reg: u16,
    amount: u16,
) -> Result<(), String> {
    let block = block_ptr.block_value()?;
    let mut new_context =
        ExecutionContext::from_block_with_receiver(&block, receiver_ptr);

    prepare_block_arguments!(context, new_context, start_reg, amount);
    process.push_context(new_context);

    Ok(())
}

#[inline(always)]
pub fn tail_call(context: &mut ExecutionContext, start_reg: u16, amount: u16) {
    context.binding.reset_locals();
    prepare_block_arguments!(context, context, start_reg, amount);
    context.registers.values.reset();

    context.instruction_index = 0;
}

#[inline(always)]
pub fn set_block(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    code_index: u16,
    receiver_ptr: ObjectPointer,
) -> ObjectPointer {
    let code = context.code.code_object(code_index as usize);
    let captures_from = if code.captures {
        Some(context.binding.clone())
    } else {
        None
    };

    let receiver = if receiver_ptr.is_null() {
        *context.binding.receiver()
    } else {
        receiver_ptr
    };

    let block = Block::new(code, captures_from, receiver, &context.module);

    process.allocate(object_value::block(block), state.block_prototype)
}
