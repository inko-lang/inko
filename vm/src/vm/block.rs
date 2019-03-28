//! VM functions for working with Inko blocks.
use crate::block::Block;
use crate::compiled_code::CompiledCodePointer;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

/// Creates a block from a CompiledCode object.
pub fn create(
    state: &RcState,
    process: &RcProcess,
    code: CompiledCodePointer,
    receiver_opt: Option<ObjectPointer>,
) -> ObjectPointer {
    let context = process.context();

    let captures_from = if code.captures {
        Some(context.binding.clone())
    } else {
        None
    };

    let receiver = receiver_opt.unwrap_or(context.binding.receiver);

    let block =
        Block::new(code, captures_from, receiver, *process.global_scope());

    process.allocate(object_value::block(block), state.block_prototype)
}

pub fn metadata(
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
        5 => {
            if block.code.rest_argument {
                state.true_object
            } else {
                state.false_object
            }
        }
        _ => {
            return Err(format!("{} is not a valid block metadata type", kind));
        }
    };

    Ok(result)
}
