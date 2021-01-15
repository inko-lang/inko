//! Functions for working with Inko blocks, such as methods and closures.
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Returns the name of a block.
///
/// This function requires a single argument: the block to get the data for.
pub fn block_name(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let block = arguments[0].block_value()?;

    Ok(block.code.name)
}

/// Returns the file path of a block.
///
/// This function requires a single argument: the block to get the data for.
pub fn block_file(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let block = arguments[0].block_value()?;

    Ok(block.code.file)
}

/// Returns the line number the block is defined on.
///
/// This function requires a single argument: the block to get the data for.
pub fn block_line(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let block = arguments[0].block_value()?;

    Ok(ObjectPointer::integer(i64::from(block.code.line)))
}

/// Returns the names of the block's arguments.
///
/// This function requires a single argument: the block to get the data for.
pub fn block_arguments(
    state: &RcState,
    process: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let block = arguments[0].block_value()?;

    Ok(process.allocate(
        object_value::array(block.code.arguments.clone()),
        state.array_prototype,
    ))
}

/// Returns the number of arguments required by the block.
///
/// This function requires a single argument: the block to get the data for.
pub fn block_required_arguments(
    _: &RcState,
    _: &RcProcess,
    arguments: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let block = arguments[0].block_value()?;

    Ok(ObjectPointer::integer(i64::from(
        block.code.required_arguments,
    )))
}

register!(
    block_name,
    block_file,
    block_line,
    block_arguments,
    block_required_arguments
);
