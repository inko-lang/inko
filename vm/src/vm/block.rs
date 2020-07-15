//! VM functions for working with Inko blocks.
use crate::block::Block;
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

macro_rules! prepare_block_arguments {
    ($old_context: expr, $new_context: expr, $start_reg: expr, $amount: expr) => {{
        if $amount > 0 {
            for (index, register) in
                ($start_reg..($start_reg + $amount)).enumerate()
            {
                $new_context.set_local(
                    index as u16,
                    $old_context.get_register(register),
                );
            }
        }
    }};
}

#[inline(always)]
pub fn block_get_receiver(context: &ExecutionContext) -> ObjectPointer {
    context.binding.receiver
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
    let mut new_context = ExecutionContext::from_block(&block);

    new_context.binding.receiver = receiver_ptr;

    prepare_block_arguments!(context, new_context, start_reg, amount);
    process.push_context(new_context);

    Ok(())
}

#[inline(always)]
pub fn tail_call(context: &mut ExecutionContext, start_reg: u16, amount: u16) {
    context.binding.locals_mut().reset();
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
        context.binding.receiver
    } else {
        receiver_ptr
    };

    let block = Block::new(code, captures_from, receiver, context.global_scope);

    process.allocate(object_value::block(block), state.block_prototype)
}

#[inline(always)]
pub fn block_metadata(
    state: &RcState,
    process: &RcProcess,
    block_ptr: ObjectPointer,
    field_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let block = block_ptr.block_value()?;
    let kind = field_ptr.integer_value()?;

    let result = match kind {
        0 => block.code.name,
        1 => block.code.file,
        2 => ObjectPointer::integer(i64::from(block.code.line)),
        3 => process.allocate(
            object_value::array(block.code.arguments.clone()),
            state.array_prototype,
        ),
        4 => ObjectPointer::integer(i64::from(block.code.required_arguments)),
        _ => {
            return Err(format!("{} is not a valid block metadata type", kind));
        }
    };

    Ok(result)
}
