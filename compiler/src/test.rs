//! Various test helper functions and types.
use crate::hir;
use crate::state::State;
use ast::source_location::SourceLocation;
use types::module_name::ModuleName;
use types::{
    Location, Module, ModuleId, Symbol, Trait, TypeRef, Visibility,
    DROP_MODULE, DROP_TRAIT,
};

pub(crate) fn cols(start: usize, stop: usize) -> SourceLocation {
    SourceLocation::new(1..=1, start..=stop)
}

pub(crate) fn hir_module(
    state: &mut State,
    name: ModuleName,
    expressions: Vec<hir::TopLevelExpression>,
) -> hir::Module {
    hir::Module {
        documentation: String::new(),
        module_id: Module::alloc(&mut state.db, name, "test.inko".into()),
        expressions,
        location: cols(1, 1),
    }
}

pub(crate) fn hir_type_name(
    name: &str,
    arguments: Vec<hir::Type>,
    location: SourceLocation,
) -> hir::TypeName {
    hir::TypeName {
        source: None,
        resolved_type: TypeRef::Unknown,
        name: hir::Constant {
            name: name.to_string(),
            location: location.clone(),
        },
        arguments,
        location,
    }
}

pub(crate) fn module_type(state: &mut State, name: &str) -> ModuleId {
    Module::alloc(
        &mut state.db,
        ModuleName::new(name),
        format!("{}.inko", name).into(),
    )
}

pub(crate) fn define_drop_trait(state: &mut State) {
    let module = Module::alloc(
        &mut state.db,
        ModuleName::new(DROP_MODULE),
        "drop.inko".into(),
    );

    let drop_trait = Trait::alloc(
        &mut state.db,
        DROP_TRAIT.to_string(),
        Visibility::Public,
        module,
        Location::default(),
    );

    module.new_symbol(
        &mut state.db,
        DROP_TRAIT.to_string(),
        Symbol::Trait(drop_trait),
    );
}
