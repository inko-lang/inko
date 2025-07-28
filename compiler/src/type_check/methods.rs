//! Passes for defining and checking method definitions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{
    define_type_bounds, DefineAndCheckTypeSignature, Rules, TypeScope,
};
use location::Location;
use std::path::PathBuf;
use types::check::{Environment, TypeChecker};
use types::format::{format_type, TypeFormatter};
use types::{
    Block, Database, Method, MethodId, MethodKind, MethodSource, ModuleId,
    Symbol, TraitId, TraitInstance, TypeArguments, TypeBounds, TypeEnum,
    TypeId, TypeInstance, TypeRef, Visibility, DROP_METHOD, MAIN_METHOD,
    MAIN_TYPE,
};

fn method_kind(kind: hir::MethodKind) -> MethodKind {
    match kind {
        hir::MethodKind::Regular => MethodKind::Instance,
        hir::MethodKind::Moving => MethodKind::Moving,
        hir::MethodKind::Mutable => MethodKind::Mutable,
    }
}

fn receiver_type(
    db: &Database,
    id: TypeEnum,
    kind: hir::MethodKind,
) -> TypeRef {
    match id {
        TypeEnum::TypeInstance(ins)
            if ins.instance_of().is_value_type(db)
                && !ins.instance_of().kind(db).is_async() =>
        {
            TypeRef::Owned(id)
        }
        _ => match kind {
            hir::MethodKind::Regular => TypeRef::Ref(id),
            hir::MethodKind::Moving => TypeRef::Owned(id),
            hir::MethodKind::Mutable => TypeRef::Mut(id),
        },
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
        receiver: TypeEnum,
    ) {
        for param_node in nodes {
            let name = &param_node.name.name;

            if let Some(Symbol::TypeParameter(_)) =
                receiver.named_type(self.db_mut(), name)
            {
                let rec_name = format_type(self.db(), receiver);
                let file = self.file();

                self.state_mut().diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!(
                        "the type parameter '{}' is already defined for '{}', \
                        and shadowing type parameters isn't allowed",
                        name, rec_name
                    ),
                    file,
                    param_node.name.location,
                );

                // We don't bail out here so we can type-check the rest of the
                // type parameters as if everything were fine.
            }

            let pid = method.new_type_parameter(
                self.db_mut(),
                param_node.name.name.clone(),
            );

            if param_node.mutable {
                pid.set_mutable(self.db_mut());
            }

            if param_node.copy {
                pid.set_copy(self.db_mut());
            }

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
            let location = Location::start_end(
                &nodes[0].location,
                &nodes.last().unwrap().location,
            );

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                format!("methods are limited to at most {} arguments", max),
                file,
                location,
            );
        }

        for node in nodes {
            let arg_type = self.type_check(&mut node.value_type, rules, scope);

            if require_send && !arg_type.is_sendable(self.db()) {
                let name = format_type(self.db(), arg_type);
                let file = self.file();

                self.state_mut().diagnostics.unsendable_async_type(
                    name,
                    file,
                    node.location,
                );
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
                node.location,
            );
        }
    }

    fn define_return_type(
        &mut self,
        node: Option<&mut hir::Type>,
        method: MethodId,
        rules: Rules,
        scope: &TypeScope,
    ) {
        let rules = rules.with_never();
        let typ = if let Some(node) = node {
            let typ = self.type_check(node, rules, scope);

            if method.is_async(self.db()) && !typ.is_sendable(self.db()) {
                let name = format_type(self.db(), typ);
                let file = self.file();

                self.state_mut().diagnostics.unsendable_async_type(
                    name,
                    file,
                    node.location(),
                );
            }

            typ
        } else {
            TypeRef::nil()
        };

        method.set_return_type(self.db_mut(), typ);
    }

    fn add_method_to_type(
        &mut self,
        method: MethodId,
        type_id: TypeId,
        name: &str,
        location: Location,
    ) {
        if type_id.method_exists(self.db(), name) {
            let tname = format_type(self.db(), type_id);
            let file = self.file();

            self.state_mut()
                .diagnostics
                .duplicate_method(name, tname, file, location);
        } else {
            type_id.add_method(self.db_mut(), name.to_string(), method);
        }
    }

    fn check_if_mutating_method_is_allowed(
        &mut self,
        kind: hir::MethodKind,
        type_id: TypeId,
        location: Location,
    ) {
        if !matches!(kind, hir::MethodKind::Mutable)
            || type_id.allow_mutating(self.db())
        {
            return;
        }

        let name = type_id.name(self.db()).clone();
        let file = self.file();

        self.state_mut().diagnostics.error(
            DiagnosticId::InvalidMethod,
            format!(
                "'{}' doesn't support mutating methods because it's an \
                immutable type",
                name
            ),
            file,
            location,
        );
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
            match expr {
                hir::TopLevelExpression::ModuleMethod(ref mut node) => {
                    self.define_module_method(node);
                }
                hir::TopLevelExpression::ExternFunction(ref mut node) => {
                    self.define_extern_function(node);
                }
                _ => (),
            }
        }
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let name = &node.name.name;
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            name.clone(),
            Visibility::public(node.public),
            MethodKind::Static,
        );

        if node.inline {
            method.always_inline(self.db_mut());
        }

        if node.c_calling_convention {
            method.use_c_calling_convention(self.db_mut());
        }

        if self.module.symbol_exists(self.db(), name) {
            self.state.diagnostics.duplicate_symbol(
                name,
                self.file(),
                node.location,
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

    fn define_extern_function(&mut self, node: &mut hir::DefineExternFunction) {
        let name = &node.name.name;
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            name.clone(),
            Visibility::public(node.public),
            MethodKind::Extern,
        );

        if node.variadic {
            method.set_variadic(self.db_mut());
        }

        if self.module.symbol_exists(self.db(), name) {
            self.state.diagnostics.duplicate_symbol(
                name,
                self.file(),
                node.location,
            );
        } else {
            self.module.new_symbol(
                self.db_mut(),
                name.clone(),
                Symbol::Method(method),
            );
        }

        node.method_id = Some(method);
        self.module.add_extern_method(self.db_mut(), method);
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
                hir::TopLevelExpression::Type(ref mut node) => {
                    self.define_type(node);
                }
                hir::TopLevelExpression::Trait(ref mut node) => {
                    self.define_trait(node);
                }
                hir::TopLevelExpression::ModuleMethod(ref mut node) => {
                    self.define_module_method(node);
                }
                hir::TopLevelExpression::ExternFunction(ref mut node) => {
                    self.define_extern_function(node);
                }
                hir::TopLevelExpression::Reopen(ref mut node) => {
                    self.reopen_type(node);
                }
                _ => {}
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let type_id = node.type_id.unwrap();

        for expr in &mut node.body {
            match expr {
                hir::TypeExpression::AsyncMethod(ref mut node) => {
                    self.define_async_method(type_id, node, TypeBounds::new());
                }
                hir::TypeExpression::StaticMethod(ref mut node) => {
                    self.define_static_method(type_id, node)
                }
                hir::TypeExpression::InstanceMethod(ref mut node) => {
                    self.define_instance_method(
                        type_id,
                        node,
                        TypeBounds::new(),
                    );
                }
                hir::TypeExpression::Constructor(ref mut node) => {
                    self.define_constructor_method(type_id, node);
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
                        "the required trait '{}' defines the method '{}', \
                        but this method is also defined in trait '{}'",
                        format_type(self.db(), requirement),
                        method.name(self.db()),
                        format_type(self.db(), trait_id),
                    ),
                    self.file(),
                    req_node.location,
                );
            }
        }
    }

    fn reopen_type(&mut self, node: &mut hir::ReopenType) {
        let tname = &node.type_name.name;
        let type_id = match self.module.use_symbol(self.db_mut(), tname) {
            Some(Symbol::Type(id)) => id,
            Some(_) => {
                self.state.diagnostics.not_a_type(
                    tname,
                    self.file(),
                    node.type_name.location,
                );

                return;
            }
            None => {
                self.state.diagnostics.undefined_symbol(
                    tname,
                    self.file(),
                    node.type_name.location,
                );

                return;
            }
        };

        if type_id.kind(self.db()).is_extern() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidImplementation,
                "methods can't be defined for extern types",
                self.file(),
                node.location,
            );
        }

        let bounds = define_type_bounds(
            self.state,
            self.module,
            type_id,
            &mut node.bounds,
        );

        for expr in &mut node.body {
            match expr {
                hir::ReopenTypeExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(type_id, n, bounds.clone());
                }
                hir::ReopenTypeExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(type_id, n);
                }
                hir::ReopenTypeExpression::AsyncMethod(ref mut n) => {
                    self.define_async_method(type_id, n, bounds.clone());
                }
            }
        }

        node.type_id = Some(type_id);
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let self_type = TypeEnum::Module(self.module);
        let receiver = TypeRef::Owned(self_type);
        let method = node.method_id.unwrap();

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::new(self.module, self_type, Some(method));
        let rules = Rules {
            allow_private_types: method.is_private(self.db()),
            // `Self` isn't allowed in module methods because there's no
            // meaningful type to replace it with.
            allow_self: false,
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
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
    }

    fn define_extern_function(&mut self, node: &mut hir::DefineExternFunction) {
        let self_type = TypeEnum::Module(self.module);
        let func = node.method_id.unwrap();
        let scope = TypeScope::new(self.module, self_type, None);
        let rules = Rules {
            allow_private_types: func.is_private(self.db()),
            ..Default::default()
        };

        for arg in &mut node.arguments {
            let name = arg.name.name.clone();
            let typ = self.type_check(&mut arg.value_type, rules, &scope);

            func.new_argument(self.db_mut(), name, typ, typ, arg.location);
        }

        let ret = node
            .return_type
            .as_mut()
            .map(|node| self.type_check(node, rules.with_never(), &scope))
            .unwrap_or_else(TypeRef::nil);

        func.set_return_type(self.db_mut(), ret);
    }

    fn define_static_method(
        &mut self,
        type_id: TypeId,
        node: &mut hir::DefineStaticMethod,
    ) {
        let receiver = TypeRef::Owned(TypeEnum::Type(type_id));
        let bounds = TypeBounds::new();
        let self_type = TypeEnum::TypeInstance(TypeInstance::for_self_type(
            self.db_mut(),
            type_id,
            &bounds,
        ));
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            node.name.name.clone(),
            Visibility::public(node.public),
            MethodKind::Static,
        );

        if node.inline {
            method.always_inline(self.db_mut());
        }

        method.set_receiver(self.db_mut(), receiver);

        let scope = TypeScope::new(self.module, self_type, Some(method));
        let rules = Rules {
            allow_private_types: type_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };

        self.define_type_parameters(
            &mut node.type_parameters,
            method,
            TypeEnum::Type(type_id),
        );
        self.define_type_parameter_requirements(
            &mut node.type_parameters,
            rules.without_self_type(),
            &scope,
        );
        self.define_arguments(
            &mut node.arguments,
            method,
            rules.without_self_type(),
            &scope,
        );
        // Return types _are_ allowed to use `Self`.
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
        self.add_method_to_type(
            method,
            type_id,
            &node.name.name,
            node.location,
        );

        node.method_id = Some(method);
    }

    fn define_instance_method(
        &mut self,
        type_id: TypeId,
        node: &mut hir::DefineInstanceMethod,
        mut bounds: TypeBounds,
    ) {
        let async_type = type_id.kind(self.db()).is_async();

        if matches!(node.kind, hir::MethodKind::Moving) && async_type {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "moving methods can't be defined for 'async' types",
                self.file(),
                node.location,
            );
        }

        self.check_if_mutating_method_is_allowed(
            node.kind,
            type_id,
            node.location,
        );

        let self_id = TypeEnum::Type(type_id);
        let module = self.module;
        let vis = if async_type {
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
            node.location,
            node.name.name.clone(),
            vis,
            kind,
        );

        if node.inline {
            method.always_inline(self.db_mut());
        }

        if !method.is_mutable_or_moving(self.db()) {
            bounds.make_immutable(self.db_mut());
        }

        // Regular instance methods on an `async` type must be private to the
        // type itself.
        if async_type && method.is_public(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "instance methods defined on 'async' types must be private",
                self.file(),
                node.location,
            );
        }

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: type_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let rec_type = TypeEnum::TypeInstance(TypeInstance::rigid(
            self.db_mut(),
            type_id,
            &bounds,
        ));
        let receiver = receiver_type(self.db(), rec_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);

        let self_type = TypeEnum::TypeInstance(TypeInstance::for_self_type(
            self.db_mut(),
            type_id,
            &bounds,
        ));
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
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );
        self.add_method_to_type(
            method,
            type_id,
            &node.name.name,
            node.location,
        );

        method.set_bounds(self.db_mut(), bounds);
        node.method_id = Some(method);
    }

    fn define_async_method(
        &mut self,
        type_id: TypeId,
        node: &mut hir::DefineAsyncMethod,
        mut bounds: TypeBounds,
    ) {
        let self_id = TypeEnum::Type(type_id);
        let module = self.module;
        let kind = if node.mutable {
            MethodKind::AsyncMutable
        } else {
            MethodKind::Async
        };
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            node.name.name.clone(),
            Visibility::public(node.public),
            kind,
        );

        if !method.is_mutable_or_moving(self.db()) {
            bounds.make_immutable(self.db_mut());
        }

        if !type_id.kind(self.db()).is_async() {
            let file = self.file();

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                "'async' methods can only be defined for 'async' types"
                    .to_string(),
                file,
                node.location,
            );
        }

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: type_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let rec_type = TypeEnum::TypeInstance(TypeInstance::rigid(
            self.db_mut(),
            type_id,
            &bounds,
        ));
        let receiver = if node.mutable {
            TypeRef::Mut(rec_type)
        } else {
            TypeRef::Ref(rec_type)
        };

        method.set_receiver(self.db_mut(), receiver);
        method.set_return_type(self.db_mut(), TypeRef::nil());

        let self_type = TypeEnum::TypeInstance(TypeInstance::for_self_type(
            self.db_mut(),
            type_id,
            &bounds,
        ));
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

        if node.return_type.is_some() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidMethod,
                "async methods can't return values",
                self.file(),
                node.location,
            );
        }

        self.add_method_to_type(
            method,
            type_id,
            &node.name.name,
            node.location,
        );

        method.set_bounds(self.db_mut(), bounds);
        node.method_id = Some(method);
    }

    fn define_required_method(
        &mut self,
        trait_id: TraitId,
        node: &mut hir::DefineRequiredMethod,
    ) {
        let name = &node.name.name;
        let self_id = TypeEnum::Trait(trait_id);
        let module = self.module;
        let kind = method_kind(node.kind);
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            name.clone(),
            Visibility::public(node.public),
            kind,
        );

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: trait_id.is_private(self.db()),
            mark_trait_for_self: true,
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let rec_ins = TraitInstance::rigid(self.db_mut(), trait_id, &bounds)
            .as_self_type();
        let rec_type = TypeEnum::TraitInstance(rec_ins);
        let receiver = receiver_type(self.db(), rec_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);

        let self_type = TypeEnum::TraitInstance(TraitInstance::for_self_type(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
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
                node.location,
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
        let self_id = TypeEnum::Trait(trait_id);
        let module = self.module;
        let kind = method_kind(node.kind);
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            name.clone(),
            Visibility::public(node.public),
            kind,
        );

        if node.inline {
            method.always_inline(self.db_mut());
        }

        self.define_type_parameters(&mut node.type_parameters, method, self_id);

        let rules = Rules {
            allow_private_types: trait_id.is_private(self.db()),
            mark_trait_for_self: true,
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let rec_ins = TraitInstance::rigid(self.db_mut(), trait_id, &bounds)
            .as_self_type();
        let receiver = receiver_type(
            self.db(),
            TypeEnum::TraitInstance(rec_ins),
            node.kind,
        );

        method.set_receiver(self.db_mut(), receiver);

        let self_type = TypeEnum::TraitInstance(TraitInstance::for_self_type(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
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
                node.location,
            );
        } else {
            trait_id.add_default_method(self.db_mut(), name.clone(), method);
        }

        node.method_id = Some(method);
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn define_constructor_method(
        &mut self,
        type_id: TypeId,
        node: &mut hir::DefineConstructor,
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
            node.location,
            name.clone(),
            Visibility::Public,
            MethodKind::Constructor,
        );

        // Constructor methods just set a bunch of fields so we can and should
        // always inline them.
        method.always_inline(self.db_mut());

        let constructor =
            type_id.constructor(self.db(), &node.name.name).unwrap();

        for (index, typ) in
            constructor.arguments(self.db()).to_vec().into_iter().enumerate()
        {
            let var_type = typ.as_rigid_type(self.db_mut(), &bounds);

            method.new_argument(
                self.db_mut(),
                format!("arg{}", index),
                var_type,
                typ,
                node.location,
            );
        }

        let stype = TypeEnum::Type(type_id);
        let rec = TypeRef::Owned(stype);
        let ret = if type_id.is_generic(self.db()) {
            let args = type_id
                .type_parameters(self.db())
                .into_iter()
                .map(|param| TypeRef::Any(TypeEnum::TypeParameter(param)))
                .collect();

            TypeInstance::with_types(self.db_mut(), type_id, args)
        } else {
            TypeInstance::new(type_id)
        };

        method.set_receiver(self.db_mut(), rec);
        method.set_return_type(
            self.db_mut(),
            TypeRef::Owned(TypeEnum::TypeInstance(ret)),
        );
        type_id.add_method(self.db_mut(), name, method);

        node.method_id = Some(method);
        node.constructor_id = Some(constructor);
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

        if let Some((typ, method)) = self.main_method(mod_id) {
            method.set_main(self.db_mut());
            self.db_mut().set_main_method(method);
            self.db_mut().set_main_type(typ);
            true
        } else {
            self.state.diagnostics.error(
                DiagnosticId::MissingMain,
                format!(
                    "this module must define the 'async' type '{}', \
                    which must define the 'async' method '{}'",
                    MAIN_TYPE, MAIN_METHOD
                ),
                mod_id.file(self.db()),
                Location::default(),
            );

            false
        }
    }

    fn main_method(&mut self, mod_id: ModuleId) -> Option<(TypeId, MethodId)> {
        let typ = if let Some(Symbol::Type(type_id)) =
            mod_id.use_symbol(self.db_mut(), MAIN_TYPE)
        {
            type_id
        } else {
            return None;
        };

        if !typ.kind(self.db()).is_async() {
            return None;
        }

        let method = typ.method(self.db(), MAIN_METHOD)?;

        if method.kind(self.db()) == MethodKind::Async
            && method.number_of_arguments(self.db()) == 0
            && method.return_type(self.db()).is_nil(self.db())
        {
            Some((typ, method))
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
        let type_ins = node.type_instance.unwrap();
        let type_id = type_ins.instance_of();
        let mut mut_error = false;
        let allow_mut = type_id.allow_mutating(self.db());

        for method in trait_id.default_methods(self.db()) {
            if method.is_mutable(self.db()) && !allow_mut && !mut_error {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidImplementation,
                    "the trait '{}' can't be implemented because it defines \
                    one or more mutating methods, and '{}' is an immutable \
                    type",
                    self.file(),
                    node.location,
                );

                mut_error = true;
            }

            if !type_id.method_exists(self.db(), method.name(self.db())) {
                continue;
            }

            let type_name = format_type(self.db(), type_id);
            let trait_name = format_type(self.db(), trait_ins);
            let method_name = format_type(self.db(), method);
            let file = self.file();

            self.state.diagnostics.error(
                DiagnosticId::InvalidImplementation,
                format!(
                    "the trait '{}' can't be implemented for '{}', as its \
                    default method '{}' is already defined for '{}'",
                    trait_name, type_name, method_name, type_name
                ),
                file,
                node.location,
            );
        }

        let bounds = type_id
            .trait_implementation(self.db(), trait_id)
            .map(|i| i.bounds.clone())
            .unwrap();

        for expr in &mut node.body {
            self.implement_method(expr, type_ins, trait_ins, bounds.clone());
        }

        for req in trait_id.required_methods(self.db()) {
            if type_ins
                .instance_of()
                .method_exists(self.db(), req.name(self.db()))
            {
                continue;
            }

            let file = self.file();
            let method_name = format_type(self.db(), req);
            let type_name = format_type(self.db(), type_ins.instance_of());

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidImplementation,
                format!(
                    "the method '{}' must be implemented for '{}'",
                    method_name, type_name
                ),
                file,
                node.location,
            );
        }

        for method in trait_id.default_methods(self.db()) {
            if type_id.method_exists(self.db(), method.name(self.db())) {
                continue;
            }

            let source = MethodSource::Inherited(trait_ins, method);
            let name = method.name(self.db()).clone();
            let module_id = type_id.module(self.db());
            let copy = method.copy_method(self.db_mut(), module_id);

            // This is needed to ensure that the receiver of the default method
            // is typed as the type that implements the trait, not as the trait
            // itself.
            let new_rec =
                method.receiver_for_type_instance(self.db(), type_ins);

            copy.set_source(self.db_mut(), source);
            copy.set_receiver(self.db_mut(), new_rec);
            type_id.add_method(self.db_mut(), name, copy);
            copy.set_bounds(self.db_mut(), bounds.clone());
        }
    }

    fn implement_method(
        &mut self,
        node: &mut hir::DefineInstanceMethod,
        type_instance: TypeInstance,
        trait_instance: TraitInstance,
        mut bounds: TypeBounds,
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
                    "the method '{}' isn't defined in the trait '{}'",
                    name, trait_name
                ),
                file,
                node.location,
            );

            return;
        };

        let is_drop = trait_instance.instance_of() == self.drop_trait
            && name == DROP_METHOD;

        // `Drop.drop` is the only exception because it may be used to e.g.
        // deallocate memory, which is an immutable type (as is the case for
        // `String.drop`).
        if !is_drop {
            self.check_if_mutating_method_is_allowed(
                node.kind,
                type_instance.instance_of(),
                node.location,
            );
        }

        let rec_type = TypeEnum::TypeInstance(type_instance);
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.location,
            name.clone(),
            Visibility::public(node.public),
            method_kind(node.kind),
        );

        if node.inline {
            method.always_inline(self.db_mut());
        }

        if !method.is_mutable_or_moving(self.db()) {
            bounds.make_immutable(self.db_mut());
        }

        self.define_type_parameters(
            &mut node.type_parameters,
            method,
            rec_type,
        );

        let rules = Rules {
            allow_private_types: type_instance
                .instance_of()
                .is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let receiver = receiver_type(self.db(), rec_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);
        method.set_source(
            self.db_mut(),
            MethodSource::Implemented(trait_instance, original),
        );

        let self_type = TypeEnum::TypeInstance(TypeInstance::for_self_type(
            self.db_mut(),
            type_instance.instance_of(),
            &bounds,
        ));
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
        self.define_return_type(
            node.return_type.as_mut(),
            method,
            rules,
            &scope,
        );

        let targs = TypeArguments::for_trait(self.db(), trait_instance);
        let mut env =
            Environment::with_self_type(targs.clone(), targs, self_type);

        if !TypeChecker::new(self.db()).check_method(method, original, &mut env)
        {
            let file = self.file();
            let lhs = TypeFormatter::with_self_type(
                self.db(),
                self_type,
                Some(&env.left),
            )
            .format(method);
            let rhs = TypeFormatter::with_self_type(
                self.db(),
                self_type,
                Some(&env.right),
            )
            .format(original);

            self.state_mut().diagnostics.error(
                DiagnosticId::InvalidMethod,
                format!("the method '{}' isn't compatible with '{}'", lhs, rhs),
                file,
                node.location,
            );
        }

        if is_drop {
            // We do this after the type-check so incorrect implementations are
            // detected properly.
            method.mark_as_destructor(self.db_mut());
        }

        method.set_bounds(self.db_mut(), bounds);

        self.add_method_to_type(
            method,
            type_instance.instance_of(),
            &node.name.name,
            node.location,
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
