//! VM functions for working with Inko modules.
use crate::block::Block;
use crate::execution_context::ExecutionContext;
use crate::module_registry::RcModuleRegistry;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

#[inline(always)]
pub fn module_load(
    process: &RcProcess,
    registry: &RcModuleRegistry,
    name_ptr: ObjectPointer,
    path_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let name = name_ptr.string_value()?;
    let path = path_ptr.string_value()?;
    let (res, block, execute) = module_load_string(registry, name, path)?;

    if execute {
        process.push_context(ExecutionContext::from_block(&block));
    }

    Ok(res)
}

#[inline(always)]
pub fn module_load_string(
    registry: &RcModuleRegistry,
    name: &str,
    path: &str,
) -> Result<(ObjectPointer, Block, bool), String> {
    let mut registry = registry.lock();
    let (module_ptr, parsed) =
        registry.load(name, path).map_err(|err| err.message())?;

    let module = module_ptr.module_value()?;

    let block =
        Block::new(module.code(), None, module_ptr, module.global_scope_ref());

    Ok((module_ptr, block, parsed))
}

#[inline(always)]
pub fn module_list(
    state: &RcState,
    registry: &RcModuleRegistry,
    process: &RcProcess,
) -> ObjectPointer {
    let modules = registry.lock().parsed();

    process.allocate(object_value::array(modules), state.array_prototype)
}

#[inline(always)]
pub fn module_get(
    state: &RcState,
    registry: &RcModuleRegistry,
    name_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let name = name_ptr.string_value()?;
    let module = registry.lock().get(name);

    if let Some(pointer) = module {
        Ok(pointer)
    } else {
        Ok(state.nil_object)
    }
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
        1 => Ok(module.path()),
        2 => Ok(module.source_path()),
        _ => Err(format!("{} is not a valid module info type", field)),
    }
}
