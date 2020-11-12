//! VM functions for working with Inko modules.
use crate::block::Block;
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn module_load(
    state: &RcState,
    process: &RcProcess,
    name_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let name = name_ptr.string_value()?;
    let (res, block, execute) = module_load_string(state, name)?;

    if execute {
        process.push_context(ExecutionContext::from_block(&block));
    }

    Ok(res)
}

#[inline(always)]
pub fn module_load_string(
    state: &RcState,
    name: &str,
) -> Result<(ObjectPointer, Block, bool), String> {
    let (mod_ptr, exec) = state.modules.lock().get_for_execution(name)?;
    let module = mod_ptr.module_value()?;
    let block = Block::new(module.code(), None, mod_ptr, module);

    Ok((mod_ptr, block, exec))
}

#[inline(always)]
pub fn module_list(state: &RcState, process: &RcProcess) -> ObjectPointer {
    let modules = state.modules.lock().list();

    process.allocate(object_value::array(modules), state.array_prototype)
}

#[inline(always)]
pub fn module_get(
    state: &RcState,
    name_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let name = name_ptr.string_value()?;

    state.modules.lock().get(name)
}

#[inline(always)]
pub fn module_info(
    module_ptr: ObjectPointer,
    field_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let field = field_ptr.integer_value()?;
    let module = module_ptr.module_value()?;

    match field {
        0 => Ok(module.name()),
        1 => Ok(module.source_path()),
        _ => Err(format!("{} is not a valid module info type", field)),
    }
}
