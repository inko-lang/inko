//! Passes for defining and checking method definitions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{
    define_type_bounds, DefineAndCheckTypeSignature, Rules, TypeScope,
};
use ast::source_location::SourceLocation;
use std::path::PathBuf;
use types::format::format_type;
use types::{
    Block, ClassId, ClassInstance, Database, Method, MethodId, MethodKind,
    MethodSource, ModuleId, Symbol, TraitId, TraitInstance, TypeBounds,
    TypeContext, TypeId, TypeRef, Visibility, DROP_METHOD, MAIN_CLASS,
    MAIN_METHOD,
};

fn method_kind(kind: hir::MethodKind) -> MethodKind {
    match kind {
        hir::MethodKind::Regular => MethodKind::Instance,
        hir::MethodKind::Moving => MethodKind::Moving,
        hir::MethodKind::Mutable => MethodKind::Mutable,
    }
}

fn receiver_type(type_id: TypeId, kind: hir::MethodKind) -> TypeRef {
    match kind {
        hir::MethodKind::Regular => TypeRef::Ref(type_id),
        hir::MethodKind::Moving => TypeRef::Owned(type_id),
        hir::MethodKind::Mutable => TypeRef::Mut(type_id),
    }
}

/// A visitor for defining methods.
trait MethodDefiner {
    fn state(&self) -> &State;
    fn state_mut(&mut self) -> &mut State;
    fn module(&self) -> ModuleId;

    fn file(&self) -> PathBuf {
        self.module().file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state().db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state_mut().db
    }

    fn define_type_parameters(
        &mut self,
        nodes: &mut Vec<hir::TypeParameter>,
        method: MethodId,
        receiver_id: TypeId,
    ) {
        for param_node in nodes {
            let name = &param_node.name.name;

            if let Some(Symbol::TypeParameter(_)) =
                receiver_id.named_type(self.db(), name)
            {
                let rec_name = format_type(self.db(), receiver_id);
                let file = self.file();

                self.state_mut().diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!(
                        "The type parameter '{}' is already defined for '{}', \
                        and shadowing type parameters isn't allowed",
                        name, rec_name
                    ),
                    file,
                    param_node.name.location.clone(),
                );

                // We don't bail out here so we can type-check the rest of the
                // type parameters as if everything were fine.
            }

            let pid = method.new_type_parameter(
                self.db_mut(),
                param_node.name.name.clone(),
            );

            param_node.type_parameter_id = Some(pid);
        }
    }

    fn type_check(
        &mut self,
        node: &mut hir::Type,
        rules: Rules,
        scope: &TypeScope,
    ) -> TypeRef {
        let module = self.module();

        DefineAndCheckTypeSignature::new(self.state_mut(), module, scope, rules)
            .define_type(node)
    }

    fn define_type_parameter_requirements(
        &mut self,
        nodes: &mut Vec<hir::TypeParameter>,
        rules: Rules,
        scope: &TypeScope,
    ) {
        for param_node in nodes {
            let param = param_node.type_parameter_id.unwrap();
            let mut requirements = Vec::new();

            for req in &mut param_node.requirements {
                let module = self.module();
                let result = DefineAndCheckTypeSignature::new(
                    self.state_mut(),
                    module,
                    scope,
                    rules,
                )
                .as_trait_instance(req);

                if let Some(instance) = result {
                    requirements.push(instance);
                }
            }

            param.add_requirements(self.db_mut(), requirements);
        }
    }

    fn define_arguments(
        &mut self,
        nodes: &mut Vec<hir::MethodArgument>,
        method: MethodId,
        rules: Rules,
        scope: &TypeScope,
    ) {
        let max = u8::MAX as usize;
        let require_send = method.is_async(self.db());
        let empty_bounds = TypeBounds::new();

        if nodes.len() > max {
            let file = self.file();
            let location = SourceLocation::start_end(
                &nodes[0].location,
                &nodes.last().unwrap().location,
            );

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                format!("Methods are limited to at most {} arguments", max),
                file,
                location,
            );
        }

        for node in nodes {
            let arg_type = self.type_check(&mut node.value_type, rules, scope);

            if require_send && !arg_type.is_sendable(self.db()) {
                let name = format_type(self.db(), arg_type);
                let file = self.file();
                let loc = node.location.clone();

                self.state_mut().diagnostics.unsendable_type(name, file, loc);
            }

            let var_type = arg_type.as_rigid_type(
                self.db_mut(),
                scope.bounds.unwrap_or(&empty_bounds),
            );

            method.new_argument(
                self.db_mut(),
                node.name.name.clone(),
                var_type,
                arg_type,
            );
        }
    }

    fn define_throw_type(
        &mut self,
        node: Option<&mut hir::Type>,
        method: MethodId,
        rules: Rules,
        scope: &TypeScope,
    ) {
        let typ = if let Some(node) = node {
            let typ = self.type_check(node, rules, scope);

            if method.is_async(self.db()) && !typ.is_sendable(self.db()) {
                let name = format_type(self.db(), typ);
                let file = self.file();
                let loc = node.location().clone();

                self.state_mut().diagnostics.unsendable_type(name, file, loc);
            }

            typ
        } else {
            TypeRef::Never
        };

        method.set_throw_type(self.db_mut(), typ);
    }

    fn define_return_type(
        &mut self,
        node: Option<&mut hir::Type>,
        method: MethodId,
        rules: Rules,
        scope: &TypeScope,
    ) {
        let typ = if let Some(node) = node {
            let typ = self.type_check(node, rules, scope);

            if method.is_async(self.db()) && !typ.is_sendable(self.db()) {
                let name = format_type(self.db(), typ);
                let file = self.file();
                let loc = node.location().clone();

                self.state_mut().diagnostics.unsendable_type(name, file, loc);
            }

            typ
        } else {
            TypeRef::nil()
        };

        method.set_return_type(self.db_mut(), typ);
    }

    fn add_method_to_class(
        &mut self,
        method: MethodId,
        class_id: ClassId,
        name: &String,
        location: &SourceLocation,
    ) {
        if class_id.method_exists(self.db(), name) {
            let class_name = format_type(self.db(), class_id);
            let file = self.file();

            self.state_mut().diagnostics.duplicate_method(
                name,
                class_name,
                file,
                location.clone(),
            );
        } else {
            class_id.add_method(self.db_mut(), name.clone(), method);
        }
    }
}

/// A compiler pass that defines the basic details for module methods.
///
/// This pass _only_ defines the methods using their name, it doesn't define the
/// arguments, return type, etc.
///
/// We need a separate pass for this so module methods exist by the time we run
/// the pass to define imported symbols.
pub(crate) struct DefineModuleMethodNames<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineModuleMethodNames<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineModuleMethodNames { state, module: module.module_id }
                .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::ModuleMethod(ref mut node) = expr {
                self.define_module_method(node);
            }
        }
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let name = &node.name.name;
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            name.clone(),
            Visibility::public(node.public),
            MethodKind::Static,
        );

        if self.module.symbol_exists(self.db(), name) {
            self.state.diagnostics.error(
                DiagnosticId::DuplicateSymbol,
                format!("The module method '{}' is already defined", name),
                self.file(),
                node.location.clone(),
            );
        } else {
            self.module.new_symbol(
                self.db_mut(),
                name.clone(),
                Symbol::Method(method),
            );
        }

        module.add_method(self.db_mut(), name.clone(), method);

        node.method_id = Some(method);
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

/// A compiler pass that defines methods on types.
pub(crate) struct DefineMethods<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineMethods<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineMethods { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expression in module.expressions.iter_mut() {
            match expression {
                hir::TopLevelExpression::Class(ref mut node) => {
                    self.define_class(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                hir::TopLevelExpression::ModuleMethod(ref mut node) => {
                    self.define_module_method(node);
                }
                hir::TopLevelExpression::Reopen(ref mut node) => {
                    self.reopen_class(node);
                }
                _ => {}
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let class_id = node.class_id.unwrap();

        for expr in &mut node.body {
            match expr {
                hir::ClassExpression::AsyncMethod(ref mut node) => {
                    self.define_async_method(class_id, node);
                }
                hir::ClassExpression::StaticMethod(ref mut node) => {
                    self.define_static_method(class_id, node)
                }
                hir::ClassExpression::InstanceMethod(ref mut node) => {
                    self.define_instance_method(
                        class_id,
                        node,
                        TypeBounds::new(),
                    );
                }
                hir::ClassExpression::Variant(ref mut node) => {
                    self.define_variant_method(class_id, node);
                }
                _ => {}
            }
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        let trait_id = node.trait_id.unwrap();

        for expr in &mut node.body {
            match expr {
                hir::TraitExpression::InstanceMethod(ref mut n) => {
                    self.define_default_method(trait_id, n);
                }
                hir::TraitExpression::RequiredMethod(ref mut n) => {
                    self.define_required_method(trait_id, n);
                }
            }
        }

        for (requirement, req_node) in trait_id
            .required_traits(self.db())
            .into_iter()
            .zip(node.requirements.iter())
        {
            let req_id = requirement.instance_of();
            let methods = req_id
                .required_methods(self.db())
                .into_iter()
                .chain(req_id.default_methods(self.db()).into_iter());

            for method in methods {
                if !trait_id.method_exists(self.db(), method.name(self.db())) {
                    continue;
                }

                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!(
                        "The required trait '{}' defines the method '{}', \
                        but this method is also defined in trait '{}'",
                        format_type(self.db(), requirement),
                        method.name(self.db()),
                        format_type(self.db(), trait_id),
                    ),
                    self.file(),
                    req_node.location.clone(),
                );
            }
        }
    }

    fn reopen_class(&mut self, node: &mut hir::ReopenClass) {
        let class_name = &node.class_name.name;
        let class_id = match self.module.symbol(self.db(), class_name) {
            Some(Symbol::Class(id)) => id,
            Some(_) => {
                self.state.diagnostics.not_a_class(
                    class_name,
                    self.file(),
                    node.class_name.location.clone(),
                );

                return;
            }
            None => {
                self.state.diagnostics.undefined_symbol(
                    class_name,
                    self.file(),
                    node.class_name.location.clone(),
                );

                return;
            }
        };

        let bounds = define_type_bounds(
            self.state,
            self.module,
            class_id,
            &mut node.bounds,
        );

        for expr in &mut node.body {
            match expr {
                hir::ReopenClassExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(class_id, n, bounds.clone());
                }
                hir::ReopenClassExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(class_id, n);
                }
                hir::ReopenClassExpression::AsyncMethod(ref mut n) => {
                    self.define_async_method(class_id, n);
                }
            }
        }

        node.class_id = Some(class_id);
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let self_type = TypeId::Module(self.module);
        let receiver = TypeRef::Owned(self_type);
        let method = node.method_id.unwrap();

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::new(self.module, self_type, Some(method));
        let rules = Rules {
            allow_private_types: method.is_private(self.db()),
            ..Default::default()
        };

        self.define_type_parameters(
            &mut node.type_parameters,
            method,
            self_type,
        );
        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
    }

    fn define_static_method(
        &mut self,
        class_id: ClassId,
        node: &mut hir::DefineStaticMethod,
    ) {
        let receiver = TypeRef::Owned(TypeId::Class(class_id));
        let bounds = TypeBounds::new();
        let self_type = TypeId::ClassInstance(ClassInstance::rigid(
            self.db_mut(),
            class_id,
            &bounds,
        ));
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.name.name.clone(),
            Visibility::public(node.public),
            MethodKind::Static,
        );

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::new(self.module, self_type, Some(method));
        let rules = Rules {
            allow_private_types: class_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };

        self.define_type_parameters(
            &mut node.type_parameters,
            method,
            TypeId::Class(class_id),
        );
        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
        self.add_method_to_class(
            method,
            class_id,
            &node.name.name,
            &node.location,
        );

        node.method_id = Some(method);
    }

    fn define_instance_method(
        &mut self,
        class_id: ClassId,
        node: &mut hir::DefineInstanceMethod,
        bounds: TypeBounds,
    ) {
        let async_class = class_id.kind(self.db()).is_async();

        if node.kind.is_moving() && async_class {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Moving methods can't be defined for async classes",
                self.file(),
                node.location.clone(),
            );
        }

        let self_id = TypeId::Class(class_id);
        let module = self.module;
        let vis = if async_class {
            if node.public {
                Visibility::Public
            } else {
                Visibility::TypePrivate
            }
        } else {
            Visibility::public(node.public)
        };
        let kind = method_kind(node.kind);
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.name.name.clone(),
            vis,
            kind,
        );

        // Regular instance methods on an `async class` must be private to the
        // class itself.
        if async_class && method.is_public(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Regular instance methods for async classes must be private",
                self.file(),
                node.location.clone(),
            );
        }

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: class_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let self_type = TypeId::ClassInstance(ClassInstance::rigid(
            self.db_mut(),
            class_id,
            &bounds,
        ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(method),
            &bounds,
        );

        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
        self.add_method_to_class(
            method,
            class_id,
            &node.name.name,
            &node.location,
        );

        method.set_bounds(self.db_mut(), bounds);
        node.method_id = Some(method);
    }

    fn define_async_method(
        &mut self,
        class_id: ClassId,
        node: &mut hir::DefineAsyncMethod,
    ) {
        let self_id = TypeId::Class(class_id);
        let module = self.module;
        let kind = if node.mutable {
            MethodKind::AsyncMutable
        } else {
            MethodKind::Async
        };
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.name.name.clone(),
            Visibility::public(node.public),
            kind,
        );

        if !class_id.kind(self.db()).is_async() {
            let file = self.file();

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Async methods can only be used in async classes".to_string(),
                file,
                node.location.clone(),
            );
        }

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: class_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let self_type = TypeId::ClassInstance(ClassInstance::rigid(
            self.db_mut(),
            class_id,
            &bounds,
        ));
        let receiver = if node.mutable {
            TypeRef::Mut(self_type)
        } else {
            TypeRef::Ref(self_type)
        };

        method.set_receiver(self.db_mut(), receiver);
        method.set_return_type(self.db_mut(), TypeRef::nil());

        let scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(method),
            &bounds,
        );

        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);

        if node.throw_type.is_some() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Async methods can't throw values",
                self.file(),
                node.location.clone(),
            );
        }

        if node.return_type.is_some() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "Async methods can't return values",
                self.file(),
                node.location.clone(),
            );
        }

        self.add_method_to_class(
            method,
            class_id,
            &node.name.name,
            &node.location,
        );

        node.method_id = Some(method);
    }

    fn define_required_method(
        &mut self,
        trait_id: TraitId,
        node: &mut hir::DefineRequiredMethod,
    ) {
        let name = &node.name.name;
        let self_id = TypeId::Trait(trait_id);
        let module = self.module;
        let kind = method_kind(node.kind);
        let method = Method::alloc(
            self.db_mut(),
            module,
            name.clone(),
            Visibility::public(node.public),
            kind,
        );

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: trait_id.is_private(self.db()),
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let self_type = TypeId::TraitInstance(TraitInstance::rigid(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(method),
            &bounds,
        );

        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );

        if trait_id.method_exists(self.db(), name) {
            self.state.diagnostics.duplicate_method(
                name,
                format_type(self.db(), trait_id),
                self.file(),
                node.location.clone(),
            );
        } else {
            trait_id.add_required_method(self.db_mut(), name.clone(), method);
        }

        node.method_id = Some(method);
    }

    fn define_default_method(
        &mut self,
        trait_id: TraitId,
        node: &mut hir::DefineInstanceMethod,
    ) {
        let name = &node.name.name;
        let self_id = TypeId::Trait(trait_id);
        let module = self.module;
        let kind = method_kind(node.kind);
        let method = Method::alloc(
            self.db_mut(),
            module,
            name.clone(),
            Visibility::public(node.public),
            kind,
        );

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: trait_id.is_private(self.db()),
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let self_type = TypeId::TraitInstance(TraitInstance::rigid(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(method),
            &bounds,
        );

        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );
        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );

        if trait_id.method_exists(self.db(), name) {
            self.state.diagnostics.duplicate_method(
                name,
                format_type(self.db(), trait_id),
                self.file(),
                node.location.clone(),
            );
        } else {
            trait_id.add_default_method(self.db_mut(), name.clone(), method);
        }

        node.method_id = Some(method);
    }

    fn define_variant_method(
        &mut self,
        class_id: ClassId,
        node: &mut hir::DefineVariant,
    ) {
        // Enums are desugared when lowering to MIR. We define the static method
        // types to construct instances here, so the type checker doesn't need
        // special knowledge of expressions such as `Option.Some(42)`.
        let module = self.module;
        let name = node.name.name.clone();
        let bounds = TypeBounds::new();
        let method = Method::alloc(
            self.db_mut(),
            module,
            name.clone(),
            Visibility::Public,
            MethodKind::Static,
        );

        let variant = class_id.variant(self.db(), &node.name.name).unwrap();

        for (index, typ) in variant.members(self.db()).into_iter().enumerate() {
            let var_type = typ.as_rigid_type(self.db_mut(), &bounds);

            method.new_argument(
                self.db_mut(),
                format!("arg{}", index),
                var_type,
                typ,
            );
        }

        let stype = TypeId::Class(class_id);
        let rec = TypeRef::Owned(stype);
        let ret = if class_id.is_generic(self.db()) {
            let args = class_id
                .type_parameters(self.db())
                .into_iter()
                .map(|param| TypeRef::Infer(TypeId::TypeParameter(param)))
                .collect();

            ClassInstance::with_types(self.db_mut(), class_id, args)
        } else {
            ClassInstance::new(class_id)
        };

        method.set_receiver(self.db_mut(), rec);
        method.set_return_type(
            self.db_mut(),
            TypeRef::Owned(TypeId::ClassInstance(ret)),
        );
        class_id.add_method(self.db_mut(), name, method);

        node.method_id = Some(method);
        node.variant_id = Some(variant);
    }
}

impl<'a> MethodDefiner for DefineMethods<'a> {
    fn state(&self) -> &State {
        self.state
    }

    fn state_mut(&mut self) -> &mut State {
        self.state
    }

    fn module(&self) -> ModuleId {
        self.module
    }
}

/// A compiler pass that checks if the `Main` process and its `main` method are
/// defined, and marks the main method accordingly.
pub(crate) struct CheckMainMethod<'a> {
    state: &'a mut State,
}

impl<'a> CheckMainMethod<'a> {
    pub(crate) fn run(state: &'a mut State) -> bool {
        CheckMainMethod { state }.check()
    }

    fn check(&mut self) -> bool {
        let main_mod = if let Some(name) = self.db().main_module() {
            name.as_str()
        } else {
            // The main module isn't defined when type-checking a specific file,
            // as said file doesn't necessarily have to be the main module (i.e.
            // we're type-checking `std/string.inko`).
            return true;
        };

        let mod_id = self.db().module(main_mod);

        if let Some((class, method)) = self.main_method(mod_id) {
            method.set_main(self.db_mut());
            self.db_mut().set_main_method(method);
            self.db_mut().set_main_class(class);
            true
        } else {
            self.state.diagnostics.error(
                DiagnosticId::MissingMain,
                format!(
                    "This module must define the async class '{}', \
                    which must define the async method '{}'",
                    MAIN_CLASS, MAIN_METHOD
                ),
                mod_id.file(self.db()),
                SourceLocation::new(1..=1, 1..=1),
            );

            false
        }
    }

    fn main_method(&self, mod_id: ModuleId) -> Option<(ClassId, MethodId)> {
        let class = if let Some(Symbol::Class(class_id)) =
            mod_id.symbol(self.db(), MAIN_CLASS)
        {
            class_id
        } else {
            return None;
        };

        if !class.kind(self.db()).is_async() {
            return None;
        }

        let method = class.method(self.db(), MAIN_METHOD)?;

        if method.kind(self.db()) == MethodKind::Async
            && method.number_of_arguments(self.db()) == 0
            && method.throw_type(self.db()).is_never(self.db())
            && method.return_type(self.db()).is_nil(self.db())
        {
            Some((class, method))
        } else {
            None
        }
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass that defines methods implemented from traits
pub(crate) struct ImplementTraitMethods<'a> {
    state: &'a mut State,
    module: ModuleId,
    drop_trait: TraitId,
}

impl<'a> ImplementTraitMethods<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        let drop_trait = state.db.drop_trait();

        for module in modules {
            ImplementTraitMethods {
                state,
                module: module.module_id,
                drop_trait,
            }
            .run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Implement(ref mut node) = expr {
                self.implement_trait(node);
            }
        }
    }

    fn implement_trait(&mut self, node: &mut hir::ImplementTrait) {
        let trait_ins = node.trait_instance.unwrap();
        let trait_id = trait_ins.instance_of();
        let class_ins = node.class_instance.unwrap();
        let class_id = class_ins.instance_of();

        for method in trait_id.default_methods(self.db()) {
            if !class_id.method_exists(self.db(), method.name(self.db())) {
                continue;
            }

            let class_name = format_type(self.db(), class_id);
            let trait_name = format_type(self.db(), trait_ins);
            let method_name = format_type(self.db(), method);
            let file = self.file();

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidImplementation,
                format!(
                    "The trait '{}' can't be implemented for '{}', as its \
                    default method '{}' is already defined for '{}'",
                    trait_name, class_name, method_name, class_name
                ),
                file,
                node.location.clone(),
            );
        }

        let bounds = class_id
            .trait_implementation(self.db(), trait_id)
            .map(|i| i.bounds.clone())
            .unwrap();

        for expr in &mut node.body {
            self.implement_method(expr, class_ins, trait_ins, &bounds);
        }

        for req in trait_id.required_methods(self.db()) {
            if class_ins
                .instance_of()
                .method_exists(self.db(), req.name(self.db()))
            {
                continue;
            }

            let file = self.file();
            let method_name = format_type(self.db(), req);
            let class_name = format_type(self.db(), class_ins.instance_of());

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidImplementation,
                format!(
                    "The method '{}' must be implemented for '{}'",
                    method_name, class_name
                ),
                file,
                node.location.clone(),
            );
        }

        for method in trait_id.default_methods(self.db()) {
            if class_id.method_exists(self.db(), method.name(self.db())) {
                continue;
            }

            let source = MethodSource::Implementation(trait_ins, method);
            let name = method.name(self.db()).clone();
            let copy = method.copy_method(self.db_mut());

            copy.set_source(self.db_mut(), source);
            class_id.add_method(self.db_mut(), name, copy);
        }
    }

    fn implement_method(
        &mut self,
        node: &mut hir::DefineInstanceMethod,
        class_instance: ClassInstance,
        trait_instance: TraitInstance,
        bounds: &TypeBounds,
    ) {
        let name = &node.name.name;
        let original = if let Some(method) =
            trait_instance.instance_of().method(self.db(), name)
        {
            method
        } else {
            let file = self.file();
            let trait_name =
                format_type(self.db(), trait_instance.instance_of());

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                format!(
                    "The method '{}' isn't defined in the trait '{}'",
                    name, trait_name
                ),
                file,
                node.location.clone(),
            );

            return;
        };

        let self_type = TypeId::ClassInstance(class_instance);
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            name.clone(),
            Visibility::public(node.public),
            method_kind(node.kind),
        );

        self.define_type_parameters(
            &mut node.type_parameters,
            method,
            self_type,
        );

        let rules = Rules {
            allow_private_types: class_instance
                .instance_of()
                .is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);
        method.set_source(
            self.db_mut(),
            MethodSource::Implementation(trait_instance, original),
        );

        let scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(method),
            bounds,
        );

        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules,
            &scope,
        );

        self.define_arguments(&mut node.arguments, method, rules, &scope);
        self.define_throw_type(node.throw_type.as_mut(), method, rules, &scope);
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );

        let mut check_ctx = TypeContext::new();

        if trait_instance.instance_of().is_generic(self.db()) {
            trait_instance
                .type_arguments(self.db())
                .copy_into(&mut check_ctx.type_arguments);
        }

        if !method.type_check(self.db_mut(), original, &mut check_ctx) {
            let file = self.file();
            let expected = format_type(self.db(), original);

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                format!(
                    "This method isn't compatible with the method '{}'",
                    expected
                ),
                file,
                node.location.clone(),
            );
        }

        if trait_instance.instance_of() == self.drop_trait
            && name == DROP_METHOD
        {
            // We do this after the type-check so incorrect implementations are
            // detected properly.
            method.mark_as_destructor(self.db_mut());
        }

        method.set_bounds(self.db_mut(), bounds.clone());

        self.add_method_to_class(
            method,
            class_instance.instance_of(),
            &node.name.name,
            &node.location,
        );

        node.method_id = Some(method);
    }
}

impl<'a> MethodDefiner for ImplementTraitMethods<'a> {
    fn state(&self) -> &State {
        self.state
    }

    fn state_mut(&mut self) -> &mut State {
        self.state
    }

    fn module(&self) -> ModuleId {
        self.module
    }
}
