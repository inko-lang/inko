//! VM functions for working with Inko modules.
use crate::block::Block;
use crate::module_registry::RcModuleRegistry;
use crate::object_pointer::ObjectPointer;
use crate::vm::state::RcState;

pub fn load(
    state: &RcState,
    registry: &RcModuleRegistry,
    path: ObjectPointer,
) -> Result<(Block, bool), String> {
    load_string(state, registry, path.string_value()?)
}

pub fn load_string(
    state: &RcState,
    registry: &RcModuleRegistry,
    path: &str,
) -> Result<(Block, bool), String> {
    let mut registry = registry.lock();
    let lookup = registry.get_or_set(path).map_err(|err| err.message())?;
    let module = lookup.module;

    let block = Block::new(
        module.code(),
        None,
        state.top_level,
        module.global_scope_ref(),
    );

    Ok((block, lookup.parsed))
}
