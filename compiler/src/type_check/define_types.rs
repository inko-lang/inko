//! Passes for defining types, their type parameters, etc.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::graph::RecursiveTypeChecker;
use crate::type_check::{
    define_type_bounds, CheckTypeSignature, DefineAndCheckTypeSignature,
    DefineTypeSignature, Rules, TypeScope,
};
use location::Location;
use std::path::PathBuf;
use types::check::TypeChecker;
use types::format::format_type;
use types::{
    Constant, Database, ModuleId, Symbol, Trait, TraitId, TraitImplementation,
    Type, TypeEnum, TypeId, TypeInstance, TypeKind, TypeRef, Visibility,
    ARRAY_INTERNAL_NAME, BYTES_MODULE, BYTE_ARRAY_TYPE, CONSTRUCTORS_LIMIT,
    ENUM_TAG_FIELD, ENUM_TAG_INDEX, MAIN_TYPE, OPTION_MODULE, OPTION_TYPE,
    RESULT_MODULE, RESULT_TYPE,
};

/// A compiler pass that defines types and traits.
///
/// This pass _only_ defines the types, it doesn't define their type parameters,
/// trait requirements, etc.
pub(crate) struct DefineTypes<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineTypes<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineTypes { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expression in module.expressions.iter_mut() {
            match expression {
                hir::TopLevelExpression::Type(ref mut node) => {
                    self.define_type(node);
                }
                hir::TopLevelExpression::ExternType(ref mut node) => {
                    self.define_extern_type(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                hir::TopLevelExpression::Constant(ref mut node) => {
                    self.define_constant(node);
                }
                _ => {}
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let name = node.name.name.clone();
        let module = self.module;
        let vis = Visibility::public(node.public);
        let loc = node.location;
        let id = if let hir::TypeKind::Builtin = node.kind {
            if !self.module.is_std(self.db()) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    "builtin types can only be defined in 'std' modules",
                    self.file(),
                    node.location,
                );
            }

            if let Some(id) = self.db().builtin_type(&name) {
                id.set_module(self.db_mut(), module);
                id
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!("'{}' isn't a valid builtin type", name),
                    self.file(),
                    node.location,
                );

                return;
            }
        } else {
            let kind = match node.kind {
                hir::TypeKind::Regular => TypeKind::Regular,
                hir::TypeKind::Async => TypeKind::Async,
                hir::TypeKind::Enum => TypeKind::Enum,
                _ => unreachable!(),
            };

            let cls = Type::alloc(
                self.db_mut(),
                name.clone(),
                kind,
                vis,
                module,
                loc,
            );

            let db = self.db_mut();

            match node.semantics {
                hir::TypeSemantics::Default => {}
                hir::TypeSemantics::Inline => cls.set_inline_storage(db),
                hir::TypeSemantics::Copy => cls.set_copy_storage(db),
            }

            cls
        };

        if self.module.symbol_exists(self.db(), &name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                node.name.location,
            );
        } else {
            self.module.new_symbol(self.db_mut(), name, Symbol::Type(id));
        }

        node.type_id = Some(id);
    }

    fn define_extern_type(&mut self, node: &mut hir::DefineExternType) {
        let name = node.name.name.clone();
        let module = self.module;
        let vis = Visibility::public(node.public);
        let loc = node.location;
        let id = Type::alloc(
            self.db_mut(),
            name.clone(),
            TypeKind::Extern,
            vis,
            module,
            loc,
        );

        if self.module.symbol_exists(self.db(), &name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                node.name.location,
            );
        } else {
            self.module.new_symbol(self.db_mut(), name, Symbol::Type(id));
        }

        node.type_id = Some(id);
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let name = node.name.name.clone();
        let module = self.module;
        let id = Trait::alloc(
            self.db_mut(),
            name.clone(),
            Visibility::public(node.public),
            module,
            Location::default(),
        );

        if self.module.symbol_exists(self.db(), &name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                node.name.location,
            );
        } else {
            self.module.new_symbol(self.db_mut(), name, Symbol::Trait(id));
        }

        node.trait_id = Some(id);
    }

    fn define_constant(&mut self, node: &mut hir::DefineConstant) {
        let name = node.name.name.clone();
        let module = self.module;

        if module.symbol_exists(self.db(), &name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                node.name.location,
            );

            return;
        }

        let db = self.db_mut();
        let vis = Visibility::public(node.public);
        let loc = node.location;
        let id = Constant::alloc(db, module, loc, name, vis, TypeRef::Unknown);

        node.constant_id = Some(id);
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that adds all trait implementations to their types.
pub(crate) struct ImplementTraits<'a> {
    state: &'a mut State,
    module: ModuleId,
    drop_trait: TraitId,
}

impl<'a> ImplementTraits<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        let drop_trait = state.db.drop_trait();

        for module in modules {
            ImplementTraits { state, module: module.module_id, drop_trait }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Implement(ref mut n) = expr {
                self.implement_trait(n);
            }
        }
    }

    fn implement_trait(&mut self, node: &mut hir::ImplementTrait) {
        let type_name = &node.type_name.name;
        let type_id = match self.module.use_symbol(self.db_mut(), type_name) {
            Some(Symbol::Type(id)) => id,
            Some(_) => {
                self.state.diagnostics.not_a_type(
                    type_name,
                    self.file(),
                    node.type_name.location,
                );

                return;
            }
            None => {
                self.state.diagnostics.undefined_symbol(
                    type_name,
                    self.file(),
                    node.type_name.location,
                );

                return;
            }
        };

        if !type_id.allow_trait_implementations(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidImplementation,
                "traits can't be implemented for this type",
                self.file(),
                node.location,
            );

            return;
        }

        let bounds = define_type_bounds(
            self.state,
            self.module,
            type_id,
            &mut node.bounds,
        );
        let type_ins = TypeInstance::rigid(self.db_mut(), type_id, &bounds);
        let scope = TypeScope::with_bounds(
            self.module,
            TypeEnum::TypeInstance(type_ins),
            None,
            &bounds,
        );

        let rules = Rules::default();
        let mut definer =
            DefineTypeSignature::new(self.state, self.module, &scope, rules);

        if let Some(instance) = definer.as_trait_instance(&mut node.trait_name)
        {
            let name = &node.trait_name.name.name;

            if type_id
                .trait_implementation(self.db(), instance.instance_of())
                .is_some()
            {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidImplementation,
                    format!(
                        "the trait '{}' is already implemented for type '{}'",
                        name, type_name
                    ),
                    self.file(),
                    node.location,
                );
            } else {
                type_id.add_trait_implementation(
                    self.db_mut(),
                    TraitImplementation { instance, bounds },
                );
            }

            if instance.instance_of() == self.drop_trait {
                if !node.bounds.is_empty() {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidImplementation,
                        "type parameter bounds can't be applied to \
                        implementations of this trait",
                        self.file(),
                        node.location,
                    );
                }

                if type_id.is_copy_type(self.db()) {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidImplementation,
                        "Drop can't be implemented for 'copy' types",
                        self.file(),
                        node.location,
                    );
                }

                type_id.mark_as_having_destructor(self.db_mut());
            }

            node.trait_instance = Some(instance);
        }

        node.type_instance = Some(type_ins);
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that defines the requirements for each trait.
pub(crate) struct DefineTraitRequirements<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineTraitRequirements<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineTraitRequirements { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Trait(ref mut n) = expr {
                self.define_trait(n);
            }
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let trait_id = node.trait_id.unwrap();
        let scope =
            TypeScope::new(self.module, TypeEnum::Trait(trait_id), None);
        let rules = Rules::default();

        for req in &mut node.requirements {
            if let Some(ins) =
                DefineTypeSignature::new(self.state, self.module, &scope, rules)
                    .as_trait_instance(req)
            {
                trait_id.add_required_trait(self.db_mut(), ins);
            }
        }
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that checks the trait requirements of each trait.
pub(crate) struct CheckTraitRequirements<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> CheckTraitRequirements<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            CheckTraitRequirements { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Trait(ref mut n) = expr {
                self.define_trait(n);
            }
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        for req in &mut node.requirements {
            CheckTypeSignature::new(self.state, self.module)
                .check_type_name(req);
        }
    }
}

/// A compiler pass that verifies if all trait implementations are correct.
pub(crate) struct CheckTraitImplementations<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> CheckTraitImplementations<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            CheckTraitImplementations { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Implement(ref n) = expr {
                self.implement_trait(n);
            }
        }
    }

    fn implement_trait(&mut self, node: &hir::ImplementTrait) {
        let type_ins = node.type_instance.unwrap();
        let trait_ins = node.trait_instance.unwrap();
        let mut checker = CheckTypeSignature::new(self.state, self.module);

        checker.check_type_name(&node.trait_name);

        for bound in &node.bounds {
            for req in &bound.requirements {
                checker.check_type_name(req);
            }
        }

        for req in trait_ins.instance_of().required_traits(self.db()) {
            let mut checker = TypeChecker::new(self.db());

            if !checker.type_implements_trait(type_ins, req) {
                self.state.diagnostics.error(
                    DiagnosticId::MissingTrait,
                    format!(
                        "the trait '{}' isn't implemented for type '{}'",
                        format_type(self.db(), req),
                        type_ins.instance_of().name(self.db())
                    ),
                    self.file(),
                    node.location,
                );
            }
        }
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }
}

/// A compiler pass that defines the fields in a type.
pub(crate) struct DefineFields<'a> {
    state: &'a mut State,
    main_module: bool,
    module: ModuleId,
}

impl<'a> DefineFields<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            let main_module = state
                .db
                .main_module()
                .map_or(false, |m| m == module.module_id.name(&state.db));

            DefineFields { state, main_module, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in &mut module.expressions {
            match expr {
                hir::TopLevelExpression::Type(ref mut node) => {
                    self.define_type(node);
                }
                hir::TopLevelExpression::ExternType(ref mut node) => {
                    self.define_extern_type(node);
                }
                _ => (),
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let type_id = node.type_id.unwrap();
        let mut id: usize = 0;
        let scope = TypeScope::new(self.module, TypeEnum::Type(type_id), None);
        let is_enum = type_id.kind(self.db()).is_enum();
        let is_copy = type_id.is_copy_type(self.db());
        let is_inline = type_id.is_inline_type(self.db());
        let is_main = self.main_module && node.name.name == MAIN_TYPE;

        for expr in &mut node.body {
            let fnode = if let hir::TypeExpression::Field(ref mut n) = expr {
                n
            } else {
                continue;
            };

            let name = fnode.name.name.clone();

            if is_main || is_enum {
                self.state.diagnostics.fields_not_allowed(
                    &node.name.name,
                    self.file(),
                    fnode.location,
                );

                break;
            }

            if type_id.field(self.db(), &name).is_some() {
                self.state.diagnostics.duplicate_field(
                    &name,
                    self.file(),
                    fnode.location,
                );

                continue;
            }

            let vis = Visibility::public(fnode.public);
            let rules = Rules {
                allow_private_types: vis.is_private(),
                ..Default::default()
            };

            let typ = DefineAndCheckTypeSignature::new(
                self.state,
                self.module,
                &scope,
                rules,
            )
            .define_type(&mut fnode.value_type);

            if is_copy && !typ.is_copy_type(self.db()) {
                self.state.diagnostics.not_a_copy_type(
                    &format_type(self.db(), typ),
                    self.file(),
                    fnode.location,
                );
            }

            if !type_id.is_public(self.db()) && vis == Visibility::Public {
                self.state
                    .diagnostics
                    .public_field_private_type(self.file(), fnode.location);
            }

            let module = self.module;
            let loc = fnode.location;
            let field = type_id.new_field(
                self.db_mut(),
                name,
                id,
                typ,
                vis,
                module,
                loc,
            );

            if fnode.mutable && (is_copy || is_inline) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    "'inline' and 'copy' types don't support mutable fields",
                    self.file(),
                    loc,
                );
            } else if fnode.mutable {
                field.set_mutable(self.db_mut());
            }

            id += 1;
            fnode.field_id = Some(field);
        }
    }

    fn define_extern_type(&mut self, node: &mut hir::DefineExternType) {
        let type_id = node.type_id.unwrap();
        let mut id: usize = 0;
        let scope = TypeScope::new(self.module, TypeEnum::Type(type_id), None);

        for node in &mut node.fields {
            let name = node.name.name.clone();

            if type_id.field(self.db(), &name).is_some() {
                self.state.diagnostics.duplicate_field(
                    &name,
                    self.file(),
                    node.location,
                );

                continue;
            }

            if node.mutable {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    "fields of 'extern' types are always mutable",
                    self.file(),
                    node.location,
                );
            }

            let vis = Visibility::public(node.public);
            let rules = Rules {
                allow_private_types: vis.is_private(),
                allow_refs: false,
                ..Default::default()
            };

            let typ = DefineAndCheckTypeSignature::new(
                self.state,
                self.module,
                &scope,
                rules,
            )
            .define_type(&mut node.value_type);

            // We can't allow heap values in external types, as that would allow
            // violating their single ownership constraints.
            if !typ.is_copy_type(self.db()) {
                self.state.diagnostics.not_a_copy_type(
                    &format_type(self.db(), typ),
                    self.file(),
                    node.value_type.location(),
                );
            }

            if !type_id.is_public(self.db()) && vis == Visibility::Public {
                self.state
                    .diagnostics
                    .public_field_private_type(self.file(), node.location);
            }

            let module = self.module;
            let loc = node.location;
            let field = type_id.new_field(
                self.db_mut(),
                name,
                id,
                typ,
                vis,
                module,
                loc,
            );

            field.set_mutable(self.db_mut());

            id += 1;
            node.field_id = Some(field);
        }
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that defines type and trait types parameters, except for
/// their requirements.
pub(crate) struct DefineTypeParameters<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineTypeParameters<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineTypeParameters { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            match expr {
                hir::TopLevelExpression::Type(ref mut node) => {
                    self.define_type(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                _ => {}
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let id = node.type_id.unwrap();
        let is_copy = id.is_copy_type(self.db());

        for param in &mut node.type_parameters {
            let name = &param.name.name;

            if id.type_parameter_exists(self.db(), name) {
                self.state.diagnostics.duplicate_type_parameter(
                    name,
                    self.module.file(self.db()),
                    param.name.location,
                );
            } else {
                let pid = id.new_type_parameter(self.db_mut(), name.clone());

                if param.mutable {
                    pid.set_mutable(self.db_mut());
                }

                if is_copy || param.copy {
                    pid.set_copy(self.db_mut());
                }

                param.type_parameter_id = Some(pid);
            }
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let id = node.trait_id.unwrap();

        for param in &mut node.type_parameters {
            let name = &param.name.name;

            if id.type_parameter_exists(self.db(), name) {
                self.state.diagnostics.duplicate_type_parameter(
                    name,
                    self.module.file(self.db()),
                    param.name.location,
                );
            } else {
                let pid = id.new_type_parameter(self.db_mut(), name.clone());

                if param.mutable {
                    pid.set_mutable(self.db_mut());
                }

                if param.copy {
                    pid.set_copy(self.db_mut());
                }

                param.type_parameter_id = Some(pid);
            }
        }
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that defines the required traits for type and trait type
/// parameters.
pub(crate) struct DefineTypeParameterRequirements<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineTypeParameterRequirements<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineTypeParameterRequirements { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            match expr {
                hir::TopLevelExpression::Type(ref mut node) => {
                    self.define_type(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                _ => {}
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let self_type = TypeEnum::Type(node.type_id.unwrap());

        self.define_requirements(&mut node.type_parameters, self_type);
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let self_type = TypeEnum::Trait(node.trait_id.unwrap());

        self.define_requirements(&mut node.type_parameters, self_type);
    }

    fn define_requirements(
        &mut self,
        parameters: &mut Vec<hir::TypeParameter>,
        self_type: TypeEnum,
    ) {
        let scope = TypeScope::new(self.module, self_type, None);
        let rules = Rules::default();

        for param in parameters {
            let param_id = param.type_parameter_id.unwrap();
            let mut requirements = Vec::new();

            for req_node in &mut param.requirements {
                if let Some(instance) = DefineTypeSignature::new(
                    self.state,
                    self.module,
                    &scope,
                    rules,
                )
                .as_trait_instance(req_node)
                {
                    requirements.push(instance);
                }
            }

            param_id.add_requirements(self.db_mut(), requirements);
        }
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that verifies if type parameters on types and traits are
/// correct.
pub(crate) struct CheckTypeParameters<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> CheckTypeParameters<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            CheckTypeParameters { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            match expr {
                hir::TopLevelExpression::Type(ref node) => {
                    self.check_type_parameters(&node.type_parameters);
                }
                hir::TopLevelExpression::Trait(ref node) => {
                    self.check_type_parameters(&node.type_parameters);
                }
                _ => {}
            }
        }
    }

    fn check_type_parameters(&mut self, nodes: &Vec<hir::TypeParameter>) {
        let mut checker = CheckTypeSignature::new(self.state, self.module);

        for node in nodes {
            for req in &node.requirements {
                checker.check_type_name(req);
            }
        }
    }
}

pub(crate) struct InsertPrelude<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> InsertPrelude<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            InsertPrelude { state, module: module.module_id }.run();
        }

        true
    }

    pub(crate) fn run(&mut self) {
        self.add_type(TypeId::int());
        self.add_type(TypeId::float());
        self.add_type(TypeId::string());
        self.add_type(TypeId::array());
        self.add_type(TypeId::boolean());
        self.add_type(TypeId::nil());

        self.import_type(BYTES_MODULE, BYTE_ARRAY_TYPE);
        self.import_type(OPTION_MODULE, OPTION_TYPE);
        self.import_type(RESULT_MODULE, RESULT_TYPE);
        self.import_type("std.map", "Map");
        self.import_method("std.process", "panic");

        // This name is used when desugaring array literals.
        self.module.new_symbol(
            self.db_mut(),
            ARRAY_INTERNAL_NAME.to_string(),
            Symbol::Type(TypeId::array()),
        );
    }

    fn add_type(&mut self, id: TypeId) {
        let name = id.name(self.db()).clone();

        if self.module.symbol_exists(self.db(), &name) {
            return;
        }

        self.module.new_symbol(self.db_mut(), name, Symbol::Type(id));
    }

    fn import_type(&mut self, module: &str, name: &str) {
        let id = self.state.db.type_in_module(module, name);

        self.add_type(id);
    }

    fn import_method(&mut self, module: &str, method: &str) {
        let mod_id = self.state.db.module(module);
        let method_id = if let Some(id) = mod_id.method(self.db(), method) {
            id
        } else {
            panic!("the method {}.{} isn't defined", module, method);
        };

        if self.module.symbol_exists(self.db(), method) {
            return;
        }

        self.module.new_symbol(
            self.db_mut(),
            method.to_string(),
            Symbol::Method(method_id),
        );
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that defines the constructors for an enum type.
pub(crate) struct DefineConstructors<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineConstructors<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineConstructors { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Type(ref mut node) = expr {
                self.define_type(node);
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let type_id = node.type_id.unwrap();
        let is_enum = type_id.kind(self.db()).is_enum();
        let is_copy = type_id.is_copy_type(self.db());
        let rules = Rules::default();
        let scope = TypeScope::new(self.module, TypeEnum::Type(type_id), None);
        let mut constructors_count = 0;
        let mut args_count = 0;

        for expr in &mut node.body {
            let node =
                if let hir::TypeExpression::Constructor(ref mut node) = expr {
                    node
                } else {
                    continue;
                };

            if !is_enum {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    "constructors can only be defined for 'enum' types",
                    self.file(),
                    node.location,
                );

                continue;
            }

            let name = &node.name.name;

            if type_id.constructor(self.db(), name).is_some() {
                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!("the constructor '{}' is already defined", name),
                    self.file(),
                    node.name.location,
                );

                continue;
            }

            let mut args = Vec::new();

            for n in node.members.iter_mut() {
                let typ = DefineAndCheckTypeSignature::new(
                    self.state,
                    self.module,
                    &scope,
                    rules,
                )
                .define_type(n);

                if is_copy && !typ.is_copy_type(self.db()) {
                    self.state.diagnostics.not_a_copy_type(
                        &format_type(self.db(), typ),
                        self.file(),
                        n.location(),
                    );
                }

                args.push(typ);
            }

            let len = args.len();

            if len > args_count {
                args_count = len;
            }

            if constructors_count == CONSTRUCTORS_LIMIT {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "enums can't define more than {} constructors",
                        CONSTRUCTORS_LIMIT
                    ),
                    self.file(),
                    node.location,
                );

                continue;
            }

            constructors_count += 1;
            type_id.new_constructor(
                self.db_mut(),
                name.to_string(),
                args,
                node.location,
            );
        }

        if is_enum {
            if constructors_count == 0 {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    "'enum' types must define at least a single constructor",
                    self.file(),
                    node.location,
                );
            }

            let module = self.module;
            let db = self.db_mut();
            let vis = Visibility::TypePrivate;
            let tag_typ = TypeRef::foreign_unsigned_int(16);
            let tag_name = ENUM_TAG_FIELD.to_string();
            let loc = type_id.location(db);

            type_id.new_field(
                db,
                tag_name,
                ENUM_TAG_INDEX,
                tag_typ,
                vis,
                module,
                loc,
            );

            for index in 0..args_count {
                let id = index + 1;

                // The type of the field is the largest constructor argument for
                // this position, but the exact type might not be known yet
                // (e.g. if it's generic). As such we define the type to be
                // Unknown and handle casting it when loading it, and when
                // generating the LLVM layouts.
                let typ = TypeRef::Unknown;

                type_id.new_field(
                    db,
                    id.to_string(),
                    id,
                    typ,
                    vis,
                    module,
                    loc,
                );
            }
        }
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that adds errors for recursive stack allocated types.
pub(crate) fn check_recursive_types(
    state: &mut State,
    modules: &[hir::Module],
) -> bool {
    for module in modules {
        for expr in &module.expressions {
            let (typ, loc) = match expr {
                hir::TopLevelExpression::Type(ref n) => {
                    let id = n.type_id.unwrap();

                    // Heap types _are_ allowed to be recursive as they can't
                    // recursive into themselves without indirection.
                    if !id.is_stack_allocated(&state.db) {
                        continue;
                    }

                    (id, n.location)
                }
                hir::TopLevelExpression::ExternType(ref n) => {
                    (n.type_id.unwrap(), n.location)
                }
                _ => continue,
            };

            // The recursion check is extracted into a separate type so we can
            // separate visiting the IR and performing the actual check.
            if !RecursiveTypeChecker::new(&state.db).is_recursive(typ) {
                continue;
            }

            state.diagnostics.error(
                DiagnosticId::InvalidType,
                "types allocated on the stack can't be recursive",
                module.module_id.file(&state.db),
                loc,
            );
        }
    }

    !state.diagnostics.has_errors()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::hir;
    use crate::modules_parser::ParsedModule;
    use crate::test::{cols, define_drop_trait};
    use ast::parser::Parser;
    use types::module_name::ModuleName;
    use types::{
        ConstantId, TraitId, TraitInstance, TypeBounds, TypeId,
        FIRST_USER_TYPE_ID,
    };

    fn get_trait(db: &mut Database, module: ModuleId, name: &str) -> TraitId {
        if let Some(Symbol::Trait(id)) = module.use_symbol(db, name) {
            id
        } else {
            panic!("expected a Trait");
        }
    }

    fn get_type(db: &mut Database, module: ModuleId, name: &str) -> TypeId {
        if let Some(Symbol::Type(id)) = module.use_symbol(db, name) {
            id
        } else {
            panic!("expected a Class");
        }
    }

    fn type_expr(module: &hir::Module) -> &hir::DefineType {
        match &module.expressions[0] {
            hir::TopLevelExpression::Type(ref node) => node,
            _ => panic!("expected a DefineClass node"),
        }
    }

    fn trait_expr(module: &hir::Module) -> &hir::DefineTrait {
        match &module.expressions[0] {
            hir::TopLevelExpression::Trait(ref node) => node,
            _ => panic!("expected a DefineTrait node"),
        }
    }

    fn parse<S: Into<String>>(state: &mut State, input: S) -> Vec<hir::Module> {
        let ast = Parser::new(input.into().into(), "test.inko".into())
            .parse()
            .expect("failed to parse the input");
        let name = ModuleName::new("test");
        let module = ParsedModule { name, ast };

        hir::LowerToHir::run_all(state, vec![module])
    }

    #[test]
    fn test_define_constant() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "let A = 1");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let sym = modules[0].module_id.use_symbol(&mut state.db, "A");
        let id = ConstantId(0);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(sym, Some(Symbol::Constant(id)));
        assert_eq!(id.value_type(&state.db), TypeRef::Unknown);
    }

    #[test]
    fn test_define_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "type A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let id = TypeId(FIRST_USER_TYPE_ID + 1);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(type_expr(&modules[0]).type_id, Some(id));

        assert_eq!(id.name(&state.db), &"A".to_string());
        assert!(!id.kind(&state.db).is_async());
        assert_eq!(
            modules[0].module_id.use_symbol(&mut state.db, "A"),
            Some(Symbol::Type(id))
        );
    }

    #[test]
    fn test_define_async_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "type async A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let id = TypeId(FIRST_USER_TYPE_ID + 1);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(type_expr(&modules[0]).type_id, Some(id));

        assert_eq!(id.name(&state.db), &"A".to_string());
        assert!(id.kind(&state.db).is_async());
        assert_eq!(
            modules[0].module_id.use_symbol(&mut state.db, "A"),
            Some(Symbol::Type(id))
        );
    }

    #[test]
    fn test_define_empty_enum_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "type enum A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));
        assert!(!DefineConstructors::run_all(&mut state, &mut modules));
        assert_eq!(state.diagnostics.iter().count(), 1);
    }

    #[test]
    fn test_define_trait() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "trait A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let id = TraitId(0);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(trait_expr(&modules[0]).trait_id, Some(id));

        assert_eq!(
            modules[0].module_id.use_symbol(&mut state.db, "A"),
            Some(Symbol::Trait(id))
        );
    }

    #[test]
    fn test_implement_trait() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "impl ToString for String {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );
        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        define_drop_trait(&mut state);

        assert!(ImplementTraits::run_all(&mut state, &mut modules));

        let imp = string.trait_implementation(&state.db, to_string).unwrap();

        assert_eq!(imp.instance.instance_of(), to_string);
    }

    #[test]
    fn test_implement_generic_trait() {
        let mut state = State::new(Config::new());
        let mut modules =
            parse(&mut state, "impl ToString[String] for String {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param =
            to_string.new_type_parameter(&mut state.db, "T".to_string());

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );
        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        define_drop_trait(&mut state);
        assert!(ImplementTraits::run_all(&mut state, &mut modules));

        let imp = string.trait_implementation(&state.db, to_string).unwrap();
        let arg =
            imp.instance.type_arguments(&state.db).unwrap().get(param).unwrap();

        assert_eq!(imp.instance.instance_of(), to_string);

        if let TypeRef::Owned(TypeEnum::TypeInstance(ins)) = arg {
            assert_eq!(ins.instance_of(), string);
        } else {
            panic!("Expected the type argument to be a type instance");
        }
    }

    #[test]
    fn test_implement_trait_with_bounds() {
        let mut state = State::new(Config::new());
        let mut modules =
            parse(&mut state, "impl ToString for Array if T: ToString {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let array = Type::alloc(
            &mut state.db,
            "Array".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param = array.new_type_parameter(&mut state.db, "T".to_string());

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );
        module.new_symbol(
            &mut state.db,
            "Array".to_string(),
            Symbol::Type(array),
        );

        define_drop_trait(&mut state);
        assert!(ImplementTraits::run_all(&mut state, &mut modules));

        let imp = array.trait_implementation(&state.db, to_string).unwrap();
        let bound = imp.bounds.get(param).unwrap();

        assert_eq!(bound.name(&state.db), param.name(&state.db));
        assert_eq!(bound.requirements(&state.db)[0].instance_of(), to_string);
    }

    #[test]
    fn test_implement_trait_with_invalid_bounds() {
        let mut state = State::new(Config::new());
        let mut modules =
            parse(&mut state, "impl ToString for Array if T: ToString {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let array = Type::alloc(
            &mut state.db,
            "Array".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );
        module.new_symbol(
            &mut state.db,
            "Array".to_string(),
            Symbol::Type(array),
        );

        define_drop_trait(&mut state);
        assert!(!ImplementTraits::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.location(), &cols(28, 28));
    }

    #[test]
    fn test_implement_trait_with_undefined_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "impl ToString for String {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );

        define_drop_trait(&mut state);
        assert!(!ImplementTraits::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.location(), &cols(19, 24));
    }

    #[test]
    fn test_implement_trait_with_invalid_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "impl ToString for String {}");
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Trait(to_string),
        );

        define_drop_trait(&mut state);
        assert!(!ImplementTraits::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidType);
        assert_eq!(error.location(), &cols(19, 24));
    }

    #[test]
    fn test_define_trait_requirements() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let mut modules = parse(&mut state, "trait Debug: ToString {}");

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );

        DefineTypes::run_all(&mut state, &mut modules);

        let debug = get_trait(&mut state.db, module, "Debug");

        assert!(DefineTraitRequirements::run_all(&mut state, &mut modules));
        assert_eq!(
            debug.required_traits(&state.db)[0].instance_of(),
            to_string
        );
    }

    #[test]
    fn test_check_valid_trait_implementation() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let to_str = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let to_str_ins = TraitInstance::new(to_str);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        string.add_trait_implementation(
            &mut state.db,
            TraitImplementation {
                instance: to_str_ins,
                bounds: TypeBounds::new(),
            },
        );

        debug.add_required_trait(&mut state.db, to_str_ins);

        let mut modules = parse(&mut state, "impl Debug for String {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );
        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        define_drop_trait(&mut state);
        DefineTypes::run_all(&mut state, &mut modules);
        ImplementTraits::run_all(&mut state, &mut modules);

        assert!(CheckTraitImplementations::run_all(&mut state, &mut modules));
    }

    #[test]
    fn test_check_invalid_trait_implementation() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let to_string_ins = TraitInstance::new(to_string);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        debug.add_required_trait(&mut state.db, to_string_ins);

        let mut modules = parse(&mut state, "impl Debug for String {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );
        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        define_drop_trait(&mut state);

        DefineTypes::run_all(&mut state, &mut modules);
        ImplementTraits::run_all(&mut state, &mut modules);

        assert!(!CheckTraitImplementations::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::MissingTrait);
        assert_eq!(error.location(), &cols(1, 24));
    }

    #[test]
    fn test_define_field() {
        let mut state = State::new(Config::new());
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let string_ins = TypeInstance::new(string);
        let mut modules =
            parse(&mut state, "type Person { let @name: String }");
        let module = ModuleId(0);

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(DefineFields::run_all(&mut state, &mut modules));

        let person = get_type(&mut state.db, module, "Person");
        let field = person.field(&state.db, "name").unwrap();

        assert_eq!(
            field.value_type(&state.db),
            TypeRef::Owned(TypeEnum::TypeInstance(string_ins))
        );
    }

    #[test]
    fn test_define_duplicate_field() {
        let mut state = State::new(Config::new());
        let string = Type::alloc(
            &mut state.db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let int = Type::alloc(
            &mut state.db,
            "Int".to_string(),
            TypeKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let mut modules = parse(
            &mut state,
            "type Person { let @name: String let @name: Int }",
        );
        let module = ModuleId(0);

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Type(string),
        );

        module.new_symbol(&mut state.db, "Int".to_string(), Symbol::Type(int));

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineFields::run_all(&mut state, &mut modules));

        let person = get_type(&mut state.db, module, "Person");
        let field = person.field(&state.db, "name").unwrap();
        let string_ins = TypeInstance::new(string);

        assert_eq!(
            field.value_type(&state.db),
            TypeRef::Owned(TypeEnum::TypeInstance(string_ins))
        );

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.location(), &cols(33, 46));
    }

    #[test]
    fn test_define_trait_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "trait A[T] {}");
        let module = ModuleId(0);

        DefineTypes::run_all(&mut state, &mut modules);

        let trait_a = get_trait(&mut state.db, module, "A");

        assert!(DefineTypeParameters::run_all(&mut state, &mut modules));

        let params = trait_a.type_parameters(&state.db);

        assert_eq!(params.len(), 1);

        let param = params[0];

        assert_eq!(param.name(&state.db), &"T");
        assert_eq!(
            trait_expr(&modules[0]).type_parameters[0].type_parameter_id,
            Some(param)
        );
    }

    #[test]
    fn test_define_duplicate_trait_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "trait A[T, T] {}");

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineTypeParameters::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(12, 12));
    }

    #[test]
    fn test_define_type_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "type A[T] {}");
        let module = ModuleId(0);

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(DefineTypeParameters::run_all(&mut state, &mut modules));

        let type_a = get_type(&mut state.db, module, "A");
        let params = type_a.type_parameters(&state.db);

        assert_eq!(params.len(), 1);

        let param = params[0];

        assert_eq!(param.name(&state.db), &"T");
        assert_eq!(
            type_expr(&modules[0]).type_parameters[0].type_parameter_id,
            Some(param)
        );
    }

    #[test]
    fn test_define_duplicate_type_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "type A[T, T] {}");

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineTypeParameters::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(11, 11));
    }

    #[test]
    fn test_define_type_type_parameter_requirements() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let mut modules = parse(&mut state, "type Array[T: Debug] {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );

        DefineTypes::run_all(&mut state, &mut modules);
        DefineTypeParameters::run_all(&mut state, &mut modules);

        assert!(DefineTypeParameterRequirements::run_all(
            &mut state,
            &mut modules
        ));

        let array = get_type(&mut state.db, module, "Array");
        let param = array.type_parameters(&state.db)[0];

        assert_eq!(param.requirements(&state.db)[0].instance_of(), debug);
    }

    #[test]
    fn test_define_trait_type_parameter_requirements() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let mut modules = parse(&mut state, "trait ToArray[T: Debug] {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );

        DefineTypes::run_all(&mut state, &mut modules);
        DefineTypeParameters::run_all(&mut state, &mut modules);

        assert!(DefineTypeParameterRequirements::run_all(
            &mut state,
            &mut modules
        ));

        let to_array = get_trait(&mut state.db, module, "ToArray");
        let param = to_array.type_parameters(&state.db)[0];

        assert_eq!(param.requirements(&state.db)[0].instance_of(), debug);
    }

    #[test]
    fn test_check_type_parameters_with_trait() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );

        debug.new_type_parameter(&mut state.db, "T".to_string());

        let mut modules = parse(&mut state, "trait ToArray[T: Debug] {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );

        DefineTypes::run_all(&mut state, &mut modules);
        DefineTypeParameters::run_all(&mut state, &mut modules);
        DefineTypeParameterRequirements::run_all(&mut state, &mut modules);

        assert!(!CheckTypeParameters::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidType);
        assert_eq!(error.location(), &cols(18, 22));
    }

    #[test]
    fn test_check_type_parameters_with_type() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );

        debug.new_type_parameter(&mut state.db, "T".to_string());

        let mut modules = parse(&mut state, "type Array[T: Debug] {}");

        module.new_symbol(
            &mut state.db,
            "Debug".to_string(),
            Symbol::Trait(debug),
        );

        DefineTypes::run_all(&mut state, &mut modules);
        DefineTypeParameters::run_all(&mut state, &mut modules);
        DefineTypeParameterRequirements::run_all(&mut state, &mut modules);

        assert!(!CheckTypeParameters::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidType);
        assert_eq!(error.location(), &cols(15, 19));
    }
}
