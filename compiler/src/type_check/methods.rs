//! Passes for defining and checking method definitions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{DefineAndCheckTypeSignature, Rules, TypeScope};
use ast::source_location::SourceLocation;
use bytecode::{BuiltinFunction as BIF, Opcode};
use std::path::PathBuf;
use types::{
    format_type, Block, BuiltinFunction, BuiltinFunctionKind, ClassId,
    ClassInstance, CompilerMacro, Database, Method, MethodId, MethodKind,
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
            MethodKind::Instance,
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
                    self.define_instance_method(class_id, node);
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

        for expr in &mut node.body {
            match expr {
                hir::ReopenClassExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(class_id, n);
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
        method.set_self_type(self.db_mut(), self_type);

        let scope = TypeScope::new(self.module, self_type, Some(method));
        let rules = Rules {
            allow_self_type: false,
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
        let self_type = TypeId::ClassInstance(
            ClassInstance::for_static_self_type(self.db_mut(), class_id),
        );
        let module = self.module;
        let method = Method::alloc(
            self.db_mut(),
            module,
            node.name.name.clone(),
            Visibility::public(node.public),
            MethodKind::Static,
        );

        method.set_receiver(self.db_mut(), receiver);
        method.set_self_type(self.db_mut(), self_type);

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
        let bounds = TypeBounds::new();
        let self_type =
            TypeId::ClassInstance(ClassInstance::for_instance_self_type(
                self.db_mut(),
                class_id,
                &bounds,
            ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);
        method.set_self_type(self.db_mut(), self_type);

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
            allow_self_type: true,
            allow_private_types: class_id.is_private(self.db())
                || method.is_private(self.db()),
            ..Default::default()
        };
        let bounds = TypeBounds::new();
        let self_type =
            TypeId::ClassInstance(ClassInstance::for_instance_self_type(
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
        method.set_self_type(self.db_mut(), self_type);

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
        let self_type = TypeId::TraitInstance(TraitInstance::for_self_type(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);
        method.set_self_type(self.db_mut(), self_type);

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
        let self_type = TypeId::TraitInstance(TraitInstance::for_self_type(
            self.db_mut(),
            trait_id,
            &bounds,
        ));
        let receiver = receiver_type(self_type, node.kind);

        method.set_receiver(self.db_mut(), receiver);
        method.set_self_type(self.db_mut(), self_type);

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

        method.set_receiver(self.db_mut(), rec);
        method.set_self_type(self.db_mut(), stype);
        method.set_return_type(self.db_mut(), TypeRef::OwnedSelf);
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
        &mut self.state
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

        if let Some(method) = self.main_method(mod_id) {
            method.set_main(self.db_mut());
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

    fn main_method(&self, mod_id: ModuleId) -> Option<MethodId> {
        let class = if let Some(Symbol::Class(class_id)) =
            mod_id.symbol(self.db(), MAIN_CLASS)
        {
            class_id
        } else {
            return None;
        };

        let stype = TypeId::ClassInstance(ClassInstance::new(class));

        if !class.kind(self.db()).is_async() {
            return None;
        }

        let method = class.method(self.db(), MAIN_METHOD)?;

        if method.kind(self.db()) == MethodKind::Async
            && method.number_of_arguments(self.db()) == 0
            && method.throw_type(self.db()).is_never(self.db())
            && method.return_type(self.db()).is_nil(self.db(), stype)
        {
            Some(method)
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

        let bounded = !node.bounds.is_empty();
        let bounds = class_id
            .trait_implementation(self.db(), trait_id)
            .map(|i| i.bounds.clone())
            .unwrap();

        for expr in &mut node.body {
            self.implement_method(expr, class_ins, trait_ins, bounded, &bounds);
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

            let source = MethodSource::implementation(bounded, trait_ins);
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
        bounded: bool,
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

        method.set_self_type(self.db_mut(), self_type);
        method.set_receiver(self.db_mut(), receiver);
        method.set_source(
            self.db_mut(),
            MethodSource::implementation(bounded, trait_instance),
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

        let mut check_ctx = TypeContext::new(self_type);

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
        &mut self.state
    }

    fn module(&self) -> ModuleId {
        self.module
    }
}

/// A compiler pass that defines all built-in function signatures.
///
/// This is just a regular function as we don't need to traverse any modules.
///
/// We set these functions up separately as some of them depend on certain types
/// (e.g. `Array`) being set up correctly.
pub(crate) fn define_builtin_functions(state: &mut State) -> bool {
    let db = &mut state.db;
    let nil = TypeRef::nil();
    let int = TypeRef::int();
    let float = TypeRef::float();
    let string = TypeRef::string();
    let any = TypeRef::Any;
    let never = TypeRef::Never;
    let boolean = TypeRef::boolean();
    let byte_array = TypeRef::byte_array();
    let string_array = TypeRef::array(db, string);

    // All the functions provided by the VM.
    let vm = vec![
        (BIF::ChildProcessDrop, any, never),
        (BIF::ChildProcessSpawn, any, int),
        (BIF::ChildProcessStderrClose, nil, never),
        (BIF::ChildProcessStderrRead, int, int),
        (BIF::ChildProcessStdinClose, nil, never),
        (BIF::ChildProcessStdinFlush, nil, int),
        (BIF::ChildProcessStdinWriteBytes, int, int),
        (BIF::ChildProcessStdinWriteString, int, int),
        (BIF::ChildProcessStdoutClose, nil, never),
        (BIF::ChildProcessStdoutRead, int, int),
        (BIF::ChildProcessTryWait, int, int),
        (BIF::ChildProcessWait, int, int),
        (BIF::EnvArguments, string_array, never),
        (BIF::EnvExecutable, string, int),
        (BIF::EnvGet, string, never),
        (BIF::EnvGetWorkingDirectory, string, int),
        (BIF::EnvHomeDirectory, string, never),
        (BIF::EnvPlatform, int, never),
        (BIF::EnvSetWorkingDirectory, nil, int),
        (BIF::EnvTempDirectory, string, never),
        (BIF::EnvVariables, string_array, never),
        (BIF::FFIFunctionAttach, any, never),
        (BIF::FFIFunctionCall, any, never),
        (BIF::FFIFunctionDrop, nil, never),
        (BIF::FFILibraryDrop, nil, never),
        (BIF::FFILibraryOpen, any, never),
        (BIF::FFIPointerAddress, int, never),
        (BIF::FFIPointerAttach, any, never),
        (BIF::FFIPointerFromAddress, any, never),
        (BIF::FFIPointerRead, any, never),
        (BIF::FFIPointerWrite, nil, never),
        (BIF::FFITypeAlignment, int, never),
        (BIF::FFITypeSize, int, never),
        (BIF::DirectoryCreate, nil, int),
        (BIF::DirectoryCreateRecursive, nil, int),
        (BIF::DirectoryList, string_array, int),
        (BIF::DirectoryRemove, nil, int),
        (BIF::DirectoryRemoveRecursive, nil, int),
        (BIF::FileCopy, int, int),
        (BIF::FileDrop, nil, never),
        (BIF::FileFlush, nil, int),
        (BIF::FileOpenAppendOnly, any, int),
        (BIF::FileOpenReadAppend, any, int),
        (BIF::FileOpenReadOnly, any, int),
        (BIF::FileOpenReadWrite, any, int),
        (BIF::FileOpenWriteOnly, any, int),
        (BIF::FileRead, int, int),
        (BIF::FileRemove, nil, int),
        (BIF::FileSeek, int, int),
        (BIF::FileSize, int, int),
        (BIF::FileWriteBytes, int, int),
        (BIF::FileWriteString, int, int),
        (BIF::PathAccessedAt, float, int),
        (BIF::PathCreatedAt, float, int),
        (BIF::PathExists, boolean, never),
        (BIF::PathIsDirectory, boolean, never),
        (BIF::PathIsFile, boolean, never),
        (BIF::PathModifiedAt, float, int),
        (BIF::HasherDrop, nil, never),
        (BIF::HasherNew, any, never),
        (BIF::HasherToHash, int, never),
        (BIF::HasherWriteInt, nil, never),
        (BIF::ProcessStacktraceDrop, nil, never),
        (BIF::ProcessCallFrameLine, int, never),
        (BIF::ProcessCallFrameName, string, never),
        (BIF::ProcessCallFramePath, string, never),
        (BIF::ProcessStacktrace, any, never),
        (BIF::RandomBytes, byte_array, never),
        (BIF::RandomFloat, float, never),
        (BIF::RandomFloatRange, float, never),
        (BIF::RandomIntRange, int, never),
        (BIF::RandomInt, int, never),
        (BIF::SocketAcceptIp, any, int),
        (BIF::SocketAcceptUnix, any, int),
        (BIF::SocketAddressPairAddress, string, never),
        (BIF::SocketAddressPairDrop, nil, never),
        (BIF::SocketAddressPairPort, int, never),
        (BIF::SocketAllocateIpv4, any, int),
        (BIF::SocketAllocateIpv6, any, int),
        (BIF::SocketAllocateUnix, any, int),
        (BIF::SocketBind, nil, int),
        (BIF::SocketConnect, nil, int),
        (BIF::SocketDrop, nil, never),
        (BIF::SocketGetBroadcast, boolean, int),
        (BIF::SocketGetKeepalive, boolean, int),
        (BIF::SocketGetLinger, float, int),
        (BIF::SocketGetNodelay, boolean, int),
        (BIF::SocketGetOnlyV6, boolean, int),
        (BIF::SocketGetRecvSize, int, int),
        (BIF::SocketGetReuseAddress, boolean, int),
        (BIF::SocketGetReusePort, boolean, int),
        (BIF::SocketGetSendSize, int, int),
        (BIF::SocketGetTtl, int, int),
        (BIF::SocketListen, nil, int),
        (BIF::SocketLocalAddress, any, int),
        (BIF::SocketPeerAddress, any, int),
        (BIF::SocketRead, int, int),
        (BIF::SocketReceiveFrom, any, int),
        (BIF::SocketSendBytesTo, int, int),
        (BIF::SocketSendStringTo, int, int),
        (BIF::SocketSetBroadcast, nil, int),
        (BIF::SocketSetKeepalive, nil, int),
        (BIF::SocketSetLinger, nil, int),
        (BIF::SocketSetNodelay, nil, int),
        (BIF::SocketSetOnlyV6, nil, int),
        (BIF::SocketSetRecvSize, nil, int),
        (BIF::SocketSetReuseAddress, nil, int),
        (BIF::SocketSetReusePort, nil, int),
        (BIF::SocketSetSendSize, nil, int),
        (BIF::SocketSetTtl, nil, int),
        (BIF::SocketShutdownRead, nil, int),
        (BIF::SocketShutdownReadWrite, nil, int),
        (BIF::SocketShutdownWrite, nil, int),
        (BIF::SocketTryClone, any, int),
        (BIF::SocketWriteBytes, int, int),
        (BIF::SocketWriteString, int, int),
        (BIF::StderrFlush, nil, int),
        (BIF::StderrWriteBytes, int, int),
        (BIF::StderrWriteString, int, int),
        (BIF::StdinRead, int, int),
        (BIF::StdoutFlush, nil, int),
        (BIF::StdoutWriteBytes, int, int),
        (BIF::StdoutWriteString, int, int),
        (BIF::TimeMonotonic, int, never),
        (BIF::TimeSystem, float, never),
        (BIF::TimeSystemOffset, int, never),
        (BIF::StringToLower, string, never),
        (BIF::StringToUpper, string, never),
        (BIF::StringToByteArray, byte_array, never),
        (BIF::StringToFloat, float, never),
        (BIF::StringToInt, int, never),
        (BIF::ByteArrayDrainToString, string, never),
        (BIF::ByteArrayToString, string, never),
        (BIF::CpuCores, int, never),
        (BIF::StringCharacters, any, never),
        (BIF::StringCharactersNext, any, never),
        (BIF::StringCharactersDrop, nil, never),
        (BIF::StringConcatArray, string, never),
        (BIF::ArrayReserve, nil, never),
        (BIF::ArrayCapacity, int, never),
        (BIF::ProcessStacktraceLength, int, never),
        (BIF::FloatToBits, int, never),
        (BIF::FloatFromBits, float, never),
        (BIF::RandomNew, any, never),
        (BIF::RandomFromInt, any, never),
        (BIF::RandomDrop, nil, never),
        (BIF::StringSliceBytes, string, never),
    ];

    // Regular VM instructions exposed directly to the standard library. These
    // are needed to implement core functionality, such as integer addition.
    let instructions = vec![
        (Opcode::ArrayClear, nil),
        (Opcode::ArrayGet, any),
        (Opcode::ArrayLength, int),
        (Opcode::ArrayPop, any),
        (Opcode::ArrayPush, nil),
        (Opcode::ArrayRemove, any),
        (Opcode::ArraySet, any),
        (Opcode::ArrayDrop, nil),
        (Opcode::ByteArrayAllocate, byte_array),
        (Opcode::ByteArrayClear, nil),
        (Opcode::ByteArrayClone, byte_array),
        (Opcode::ByteArrayEquals, boolean),
        (Opcode::ByteArrayGet, int),
        (Opcode::ByteArrayLength, int),
        (Opcode::ByteArrayPop, int),
        (Opcode::ByteArrayPush, nil),
        (Opcode::ByteArrayRemove, int),
        (Opcode::ByteArraySet, int),
        (Opcode::ByteArrayDrop, nil),
        (Opcode::Exit, never),
        (Opcode::FloatAdd, float),
        (Opcode::FloatCeil, float),
        (Opcode::FloatClone, float),
        (Opcode::FloatDiv, float),
        (Opcode::FloatEq, boolean),
        (Opcode::FloatFloor, float),
        (Opcode::FloatGe, boolean),
        (Opcode::FloatGt, boolean),
        (Opcode::FloatIsInf, boolean),
        (Opcode::FloatIsNan, boolean),
        (Opcode::FloatLe, boolean),
        (Opcode::FloatLt, boolean),
        (Opcode::FloatMod, float),
        (Opcode::FloatMul, float),
        (Opcode::FloatRound, float),
        (Opcode::FloatSub, float),
        (Opcode::FloatToInt, int),
        (Opcode::FloatToString, string),
        (Opcode::FutureDrop, any),
        (Opcode::GetNil, nil),
        (Opcode::GetUndefined, any),
        (Opcode::IntAdd, int),
        (Opcode::IntBitAnd, int),
        (Opcode::IntBitOr, int),
        (Opcode::IntBitXor, int),
        (Opcode::IntClone, int),
        (Opcode::IntDiv, int),
        (Opcode::IntEq, boolean),
        (Opcode::IntGe, boolean),
        (Opcode::IntGt, boolean),
        (Opcode::IntLe, boolean),
        (Opcode::IntLt, boolean),
        (Opcode::IntMod, int),
        (Opcode::IntMul, int),
        (Opcode::IntShl, int),
        (Opcode::IntShr, int),
        (Opcode::IntSub, int),
        (Opcode::IntPow, int),
        (Opcode::IntToFloat, float),
        (Opcode::IntToString, string),
        (Opcode::ObjectEq, boolean),
        (Opcode::Panic, never),
        (Opcode::StringByte, int),
        (Opcode::StringEq, boolean),
        (Opcode::StringSize, int),
        (Opcode::StringDrop, nil),
        (Opcode::IsUndefined, boolean),
        (Opcode::ProcessSuspend, nil),
        (Opcode::FuturePoll, any),
        (Opcode::IntBitNot, int),
        (Opcode::IntRotateLeft, int),
        (Opcode::IntRotateRight, int),
        (Opcode::IntWrappingAdd, int),
        (Opcode::IntWrappingSub, int),
        (Opcode::IntWrappingMul, int),
    ];

    let macros = vec![
        (CompilerMacro::FutureGet, any, any),
        (CompilerMacro::FutureGetFor, any, any),
        (CompilerMacro::StringClone, string, never),
        (CompilerMacro::Moved, any, never),
        (CompilerMacro::PanicThrown, never, never),
        (CompilerMacro::Strings, string, never),
    ];

    for (id, returns, throws) in vm.into_iter() {
        let kind = BuiltinFunctionKind::Function(id);

        BuiltinFunction::alloc(db, kind, id.name(), returns, throws);
    }

    for (opcode, returns) in instructions.into_iter() {
        let kind = BuiltinFunctionKind::Instruction(opcode);

        BuiltinFunction::alloc(db, kind, opcode.name(), returns, never);
    }

    for (mac, returns, throws) in macros.into_iter() {
        let kind = BuiltinFunctionKind::Macro(mac);

        BuiltinFunction::alloc(db, kind, mac.name(), returns, throws);
    }

    true
}
