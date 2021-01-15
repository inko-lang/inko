//! VM functions for handling external functions.
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

#[inline(always)]
pub fn external_function_call(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    func_ptr: ObjectPointer,
    start_reg: u16,
    amount: u16,
) -> Result<ObjectPointer, RuntimeError> {
    let func = func_ptr.external_function_value()?;
    let args = context.registers.slice(start_reg, amount);

    func(state, process, args)
}

#[inline(always)]
pub fn external_function_load(
    state: &RcState,
    name_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let name = name_ptr.string_value()?;
    let func = state.external_functions.get(name)?;
    let obj = state.permanent_allocator.lock().allocate_with_prototype(
        object_value::external_function(func),
        state.block_prototype,
    );

    Ok(obj)
}
