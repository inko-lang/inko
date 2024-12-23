//! Passes for defining types, their type parameters, etc.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::graph::RecursiveClassChecker;
use crate::type_check::{
    define_type_bounds, CheckTypeSignature, DefineAndCheckTypeSignature,
    DefineTypeSignature, Rules, TypeScope,
};
use location::Location;
use std::path::PathBuf;
use types::check::TypeChecker;
use types::format::format_type;
use types::{
    Class, ClassId, ClassInstance, ClassKind, Constant, Database, ModuleId,
    Symbol, Trait, TraitId, TraitImplementation, TypeId, TypeRef, Visibility,
    ARRAY_INTERNAL_NAME, CONSTRUCTORS_LIMIT, ENUM_TAG_FIELD, ENUM_TAG_INDEX,
    MAIN_CLASS, OPTION_CLASS, OPTION_MODULE, RESULT_CLASS, RESULT_MODULE,
};

/// A compiler pass that defines classes and traits.
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
                hir::TopLevelExpression::Class(ref mut node) => {
                    self.define_class(node);
                }
                hir::TopLevelExpression::ExternClass(ref mut node) => {
                    self.define_extern_class(node);
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

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let name = node.name.name.clone();
        let module = self.module;
        let vis = Visibility::public(node.public);
        let loc = node.location;
        let id = if let hir::ClassKind::Builtin = node.kind {
            if !self.module.is_std(self.db()) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    "builtin classes can only be defined in 'std' modules",
                    self.file(),
                    node.location,
                );
            }

            if let Some(id) = self.db().builtin_class(&name) {
                id.set_module(self.db_mut(), module);
                id
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!("'{}' isn't a valid builtin class", name),
                    self.file(),
                    node.location,
                );

                return;
            }
        } else {
            let kind = match node.kind {
                hir::ClassKind::Regular => ClassKind::Regular,
                hir::ClassKind::Async => ClassKind::Async,
                hir::ClassKind::Enum => ClassKind::Enum,
                _ => unreachable!(),
            };

            let cls = Class::alloc(
                self.db_mut(),
                name.clone(),
                kind,
                vis,
                module,
                loc,
            );

            match node.semantics {
                hir::ClassSemantics::Default => {}
                hir::ClassSemantics::Inline => {
                    cls.set_inline_storage(self.db_mut());
                }
                hir::ClassSemantics::Copy => {
                    cls.set_copy_storage(self.db_mut());
                }
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
            self.module.new_symbol(self.db_mut(), name, Symbol::Class(id));
        }

        node.class_id = Some(id);
    }

    fn define_extern_class(&mut self, node: &mut hir::DefineExternClass) {
        let name = node.name.name.clone();
        let module = self.module;
        let vis = Visibility::public(node.public);
        let loc = node.location;
        let id = Class::alloc(
            self.db_mut(),
            name.clone(),
            ClassKind::Extern,
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
            self.module.new_symbol(self.db_mut(), name, Symbol::Class(id));
        }

        node.class_id = Some(id);
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

/// A compiler pass that adds all trait implementations to their classes.
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
        let class_name = &node.class_name.name;
        let class_id = match self.module.use_symbol(self.db_mut(), class_name) {
            Some(Symbol::Class(id)) => id,
            Some(_) => {
                self.state.diagnostics.not_a_class(
                    class_name,
                    self.file(),
                    node.class_name.location,
                );

                return;
            }
            None => {
                self.state.diagnostics.undefined_symbol(
                    class_name,
                    self.file(),
                    node.class_name.location,
                );

                return;
            }
        };

        if !class_id.allow_trait_implementations(self.db()) {
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
            class_id,
            &mut node.bounds,
        );
        let class_ins = ClassInstance::rigid(self.db_mut(), class_id, &bounds);
        let scope = TypeScope::with_bounds(
            self.module,
            TypeId::ClassInstance(class_ins),
            None,
            &bounds,
        );

        let rules = Rules::default();
        let mut definer =
            DefineTypeSignature::new(self.state, self.module, &scope, rules);

        if let Some(instance) = definer.as_trait_instance(&mut node.trait_name)
        {
            let name = &node.trait_name.name.name;

            if class_id
                .trait_implementation(self.db(), instance.instance_of())
                .is_some()
            {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidImplementation,
                    format!(
                        "the trait '{}' is already implemented for class '{}'",
                        name, class_name
                    ),
                    self.file(),
                    node.location,
                );
            } else {
                class_id.add_trait_implementation(
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

                if class_id.is_copy_type(self.db()) {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidImplementation,
                        "Drop can't be implemented for 'copy' types",
                        self.file(),
                        node.location,
                    );
                }

                class_id.mark_as_having_destructor(self.db_mut());
            }

            node.trait_instance = Some(instance);
        }

        node.class_instance = Some(class_ins);
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
        let scope = TypeScope::new(self.module, TypeId::Trait(trait_id), None);
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
        let class_ins = node.class_instance.unwrap();
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

            if !checker.class_implements_trait(class_ins, req) {
                self.state.diagnostics.error(
                    DiagnosticId::MissingTrait,
                    format!(
                        "the trait '{}' isn't implemented for class '{}'",
                        format_type(self.db(), req),
                        class_ins.instance_of().name(self.db())
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

/// A compiler pass that defines the fields in a class.
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
                hir::TopLevelExpression::Class(ref mut node) => {
                    self.define_class(node);
                }
                hir::TopLevelExpression::ExternClass(ref mut node) => {
                    self.define_extern_class(node);
                }
                _ => (),
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let class_id = node.class_id.unwrap();
        let mut id: usize = 0;
        let scope = TypeScope::new(self.module, TypeId::Class(class_id), None);
        let is_enum = class_id.kind(self.db()).is_enum();
        let is_copy = class_id.is_copy_type(self.db());
        let is_main = self.main_module && node.name.name == MAIN_CLASS;

        for expr in &mut node.body {
            let fnode = if let hir::ClassExpression::Field(ref mut n) = expr {
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

            if class_id.field(self.db(), &name).is_some() {
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

            if !class_id.is_public(self.db()) && vis == Visibility::Public {
                self.state
                    .diagnostics
                    .public_field_private_class(self.file(), fnode.location);
            }

            let module = self.module;
            let loc = fnode.location;
            let field = class_id.new_field(
                self.db_mut(),
                name,
                id,
                typ,
                vis,
                module,
                loc,
            );

            id += 1;
            fnode.field_id = Some(field);
        }
    }

    fn define_extern_class(&mut self, node: &mut hir::DefineExternClass) {
        let class_id = node.class_id.unwrap();
        let mut id: usize = 0;
        let scope = TypeScope::new(self.module, TypeId::Class(class_id), None);

        for node in &mut node.fields {
            let name = node.name.name.clone();

            if class_id.field(self.db(), &name).is_some() {
                self.state.diagnostics.duplicate_field(
                    &name,
                    self.file(),
                    node.location,
                );

                continue;
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

            // We can't allow heap values in external classes, as that would
            // allow violating their single ownership constraints.
            if !typ.is_copy_type(self.db()) {
                self.state.diagnostics.not_a_copy_type(
                    &format_type(self.db(), typ),
                    self.file(),
                    node.value_type.location(),
                );
            }

            if !class_id.is_public(self.db()) && vis == Visibility::Public {
                self.state
                    .diagnostics
                    .public_field_private_class(self.file(), node.location);
            }

            let module = self.module;
            let loc = node.location;
            let field = class_id.new_field(
                self.db_mut(),
                name,
                id,
                typ,
                vis,
                module,
                loc,
            );

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

/// A compiler pass that defines class and trait types parameters, except for
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
                hir::TopLevelExpression::Class(ref mut node) => {
                    self.define_class(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                _ => {}
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let id = node.class_id.unwrap();
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

/// A compiler pass that defines the required traits for class and trait type
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
                hir::TopLevelExpression::Class(ref mut node) => {
                    self.define_class(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                _ => {}
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let self_type = TypeId::Class(node.class_id.unwrap());

        self.define_requirements(&mut node.type_parameters, self_type);
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let self_type = TypeId::Trait(node.trait_id.unwrap());

        self.define_requirements(&mut node.type_parameters, self_type);
    }

    fn define_requirements(
        &mut self,
        parameters: &mut Vec<hir::TypeParameter>,
        self_type: TypeId,
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

/// A compiler pass that verifies if type parameters on classes and traits are
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
                hir::TopLevelExpression::Class(ref node) => {
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
        self.add_class(ClassId::int());
        self.add_class(ClassId::float());
        self.add_class(ClassId::string());
        self.add_class(ClassId::array());
        self.add_class(ClassId::boolean());
        self.add_class(ClassId::nil());
        self.add_class(ClassId::byte_array());

        self.import_class(OPTION_MODULE, OPTION_CLASS);
        self.import_class(RESULT_MODULE, RESULT_CLASS);
        self.import_class("std.map", "Map");
        self.import_method("std.process", "panic");

        // This name is used when desugaring array literals.
        self.module.new_symbol(
            self.db_mut(),
            ARRAY_INTERNAL_NAME.to_string(),
            Symbol::Class(ClassId::array()),
        );
    }

    fn add_class(&mut self, id: ClassId) {
        let name = id.name(self.db()).clone();

        if self.module.symbol_exists(self.db(), &name) {
            return;
        }

        self.module.new_symbol(self.db_mut(), name, Symbol::Class(id));
    }

    fn import_class(&mut self, module: &str, class: &str) {
        let id = self.state.db.class_in_module(module, class);

        self.add_class(id);
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

/// A compiler pass that defines the constructors for an enum class.
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
            if let hir::TopLevelExpression::Class(ref mut node) = expr {
                self.define_class(node);
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let class_id = node.class_id.unwrap();
        let is_enum = class_id.kind(self.db()).is_enum();
        let is_copy = class_id.is_copy_type(self.db());
        let rules = Rules::default();
        let scope = TypeScope::new(self.module, TypeId::Class(class_id), None);
        let mut constructors_count = 0;
        let mut args_count = 0;

        for expr in &mut node.body {
            let node =
                if let hir::ClassExpression::Constructor(ref mut node) = expr {
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

            if class_id.constructor(self.db(), name).is_some() {
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
            class_id.new_constructor(
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
            let loc = class_id.location(db);

            class_id.new_field(
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

                class_id.new_field(
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

/// A compiler pass that adds errors for recursive stack allocated classes.
pub(crate) fn check_recursive_types(
    state: &mut State,
    modules: &[hir::Module],
) -> bool {
    for module in modules {
        for expr in &module.expressions {
            let (class, loc) = match expr {
                hir::TopLevelExpression::Class(ref n) => {
                    let id = n.class_id.unwrap();

                    // Heap types _are_ allowed to be recursive as they can't
                    // recursive into themselves without indirection.
                    if !id.is_stack_allocated(&state.db) {
                        continue;
                    }

                    (id, n.location)
                }
                hir::TopLevelExpression::ExternClass(ref n) => {
                    (n.class_id.unwrap(), n.location)
                }
                _ => continue,
            };

            // The recursion check is extracted into a separate type so we can
            // separate visiting the IR and performing the actual check.
            if !RecursiveClassChecker::new(&state.db).is_recursive(class) {
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
        ClassId, ConstantId, TraitId, TraitInstance, TypeBounds,
        FIRST_USER_CLASS_ID,
    };

    fn get_trait(db: &mut Database, module: ModuleId, name: &str) -> TraitId {
        if let Some(Symbol::Trait(id)) = module.use_symbol(db, name) {
            id
        } else {
            panic!("expected a Trait");
        }
    }

    fn get_class(db: &mut Database, module: ModuleId, name: &str) -> ClassId {
        if let Some(Symbol::Class(id)) = module.use_symbol(db, name) {
            id
        } else {
            panic!("expected a Class");
        }
    }

    fn class_expr(module: &hir::Module) -> &hir::DefineClass {
        match &module.expressions[0] {
            hir::TopLevelExpression::Class(ref node) => node,
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
    fn test_define_class() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let id = ClassId(FIRST_USER_CLASS_ID + 1);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(class_expr(&modules[0]).class_id, Some(id));

        assert_eq!(id.name(&state.db), &"A".to_string());
        assert!(!id.kind(&state.db).is_async());
        assert_eq!(
            modules[0].module_id.use_symbol(&mut state.db, "A"),
            Some(Symbol::Class(id))
        );
    }

    #[test]
    fn test_define_async_class() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class async A {}");

        assert!(DefineTypes::run_all(&mut state, &mut modules));

        let id = ClassId(FIRST_USER_CLASS_ID + 1);

        assert_eq!(state.diagnostics.iter().count(), 0);
        assert_eq!(class_expr(&modules[0]).class_id, Some(id));

        assert_eq!(id.name(&state.db), &"A".to_string());
        assert!(id.kind(&state.db).is_async());
        assert_eq!(
            modules[0].module_id.use_symbol(&mut state.db, "A"),
            Some(Symbol::Class(id))
        );
    }

    #[test]
    fn test_define_empty_enum_class() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class enum A {}");

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
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(string),
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
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(string),
        );

        define_drop_trait(&mut state);
        assert!(ImplementTraits::run_all(&mut state, &mut modules));

        let imp = string.trait_implementation(&state.db, to_string).unwrap();
        let arg =
            imp.instance.type_arguments(&state.db).unwrap().get(param).unwrap();

        assert_eq!(imp.instance.instance_of(), to_string);

        if let TypeRef::Owned(TypeId::ClassInstance(ins)) = arg {
            assert_eq!(ins.instance_of(), string);
        } else {
            panic!("Expected the type argument to be a class instance");
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
        let array = Class::alloc(
            &mut state.db,
            "Array".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(array),
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
        let array = Class::alloc(
            &mut state.db,
            "Array".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(array),
        );

        define_drop_trait(&mut state);
        assert!(!ImplementTraits::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.location(), &cols(28, 28));
    }

    #[test]
    fn test_implement_trait_with_undefined_class() {
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
    fn test_implement_trait_with_invalid_class() {
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
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(string),
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
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
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
            Symbol::Class(string),
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
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let string_ins = ClassInstance::new(string);
        let mut modules =
            parse(&mut state, "class Person { let @name: String }");
        let module = ModuleId(0);

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Class(string),
        );

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(DefineFields::run_all(&mut state, &mut modules));

        let person = get_class(&mut state.db, module, "Person");
        let field = person.field(&state.db, "name").unwrap();

        assert_eq!(
            field.value_type(&state.db),
            TypeRef::Owned(TypeId::ClassInstance(string_ins))
        );
    }

    #[test]
    fn test_define_duplicate_field() {
        let mut state = State::new(Config::new());
        let string = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let int = Class::alloc(
            &mut state.db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );
        let mut modules = parse(
            &mut state,
            "class Person { let @name: String let @name: Int }",
        );
        let module = ModuleId(0);

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Class(string),
        );

        module.new_symbol(&mut state.db, "Int".to_string(), Symbol::Class(int));

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineFields::run_all(&mut state, &mut modules));

        let person = get_class(&mut state.db, module, "Person");
        let field = person.field(&state.db, "name").unwrap();
        let string_ins = ClassInstance::new(string);

        assert_eq!(
            field.value_type(&state.db),
            TypeRef::Owned(TypeId::ClassInstance(string_ins))
        );

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.location(), &cols(34, 47));
    }

    #[test]
    fn test_define_field_with_self_type() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class Person { let @name: Self }");

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineFields::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.location(), &cols(27, 30));
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
    fn test_define_class_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class A[T] {}");
        let module = ModuleId(0);

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(DefineTypeParameters::run_all(&mut state, &mut modules));

        let class_a = get_class(&mut state.db, module, "A");
        let params = class_a.type_parameters(&state.db);

        assert_eq!(params.len(), 1);

        let param = params[0];

        assert_eq!(param.name(&state.db), &"T");
        assert_eq!(
            class_expr(&modules[0]).type_parameters[0].type_parameter_id,
            Some(param)
        );
    }

    #[test]
    fn test_define_duplicate_class_type_parameter() {
        let mut state = State::new(Config::new());
        let mut modules = parse(&mut state, "class A[T, T] {}");

        DefineTypes::run_all(&mut state, &mut modules);

        assert!(!DefineTypeParameters::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(12, 12));
    }

    #[test]
    fn test_define_class_type_parameter_requirements() {
        let mut state = State::new(Config::new());
        let module = ModuleId(0);
        let debug = Trait::alloc(
            &mut state.db,
            "Debug".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let mut modules = parse(&mut state, "class Array[T: Debug] {}");

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

        let array = get_class(&mut state.db, module, "Array");
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
    fn test_check_type_parameters_with_class() {
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

        let mut modules = parse(&mut state, "class Array[T: Debug] {}");

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
        assert_eq!(error.location(), &cols(16, 20));
    }
}
