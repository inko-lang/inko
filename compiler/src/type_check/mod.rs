//! Types and methods for common type-checking operations.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use ast::source_location::SourceLocation;
use std::path::PathBuf;
use types::check::{Environment, TypeChecker};
use types::format::format_type;
use types::{
    Block, ClassId, ClassInstance, Closure, Database, MethodId, ModuleId,
    Symbol, TraitId, TraitInstance, TypeArguments, TypeBounds, TypeId,
    TypeParameter, TypeParameterId, TypeRef,
};

pub(crate) mod define_types;
pub(crate) mod expressions;
pub(crate) mod imports;
pub(crate) mod methods;

#[derive(Eq, PartialEq)]
enum RefKind {
    Default,
    Owned,
    Ref,
    Mut,
    Uni,
}

impl RefKind {
    fn into_type_ref(self, id: TypeId) -> TypeRef {
        match self {
            Self::Default => match id {
                TypeId::TypeParameter(_) | TypeId::RigidTypeParameter(_) => {
                    TypeRef::Any(id)
                }
                _ => TypeRef::Owned(id),
            },
            Self::Owned => TypeRef::Owned(id),
            Self::Ref => TypeRef::Ref(id),
            Self::Mut => TypeRef::Mut(id),
            Self::Uni => TypeRef::Uni(id),
        }
    }
}

/// Data to expose to the various visitors that define and check types.
pub(crate) struct TypeScope<'a> {
    /// The surrounding module.
    module: ModuleId,

    /// The surrounding class or trait.
    self_type: TypeId,

    /// The surrounding method, if any.
    method: Option<MethodId>,

    /// Any extra type parameter bounds to apply.
    bounds: Option<&'a TypeBounds>,
}

impl<'a> TypeScope<'a> {
    pub(crate) fn new(
        module: ModuleId,
        self_type: TypeId,
        method: Option<MethodId>,
    ) -> Self {
        Self { module, self_type, method, bounds: None }
    }

    pub(crate) fn with_bounds(
        module: ModuleId,
        self_type: TypeId,
        method: Option<MethodId>,
        bounds: &'a TypeBounds,
    ) -> Self {
        Self { module, self_type, method, bounds: Some(bounds) }
    }

    pub(crate) fn symbol(&self, db: &Database, name: &str) -> Option<Symbol> {
        if let Some(id) = self.method {
            if let Some(sym) = id.named_type(db, name) {
                return Some(sym);
            }

            match self.self_type.named_type(db, name) {
                Some(Symbol::TypeParameter(pid)) => {
                    if let Some(bound) = id.bounds(db).get(pid) {
                        Some(Symbol::TypeParameter(bound))
                    } else {
                        Some(Symbol::TypeParameter(pid))
                    }
                }
                None => self.module.symbol(db, name),
                sym => sym,
            }
        } else {
            self.self_type
                .named_type(db, name)
                .or_else(|| self.module.symbol(db, name))
        }
    }
}

/// Rules to apply when defining and checking the types of type signatures.
#[derive(Copy, Clone)]
pub(crate) struct Rules {
    /// When set to `true`, type parameters are defined as rigid parameters.
    pub(crate) type_parameters_as_rigid: bool,

    /// If private types are allowed.
    pub(crate) allow_private_types: bool,

    /// If references are allowed.
    pub(crate) allow_refs: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            type_parameters_as_rigid: false,
            allow_private_types: true,
            allow_refs: true,
        }
    }
}

/// A visitor that defines the structures for types used in a type signature
/// (e.g. the list of type parameter requirements).
///
/// This visitor only defines types, it doesn't (unless strictly necessary)
/// check if a type is also valid. For example, when processing type arguments
/// this visitor doesn't check if the arguments can actually be assigned to
/// their corresponding type parameters.
pub(crate) struct DefineTypeSignature<'a> {
    state: &'a mut State,
    module: ModuleId,
    scope: &'a TypeScope<'a>,
    rules: Rules,
}

impl<'a> DefineTypeSignature<'a> {
    pub(crate) fn new(
        state: &'a mut State,
        module: ModuleId,
        scope: &'a TypeScope<'a>,
        rules: Rules,
    ) -> Self {
        Self { state, module, scope, rules }
    }

    pub(crate) fn as_trait_instance(
        &mut self,
        node: &mut hir::TypeName,
    ) -> Option<TraitInstance> {
        match self.define_type_name(node, RefKind::Owned) {
            TypeRef::Owned(TypeId::TraitInstance(instance)) => Some(instance),
            TypeRef::Error => None,
            _ => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!("'{}' isn't a trait", node.name.name),
                    self.file(),
                    node.location.clone(),
                );

                None
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::Type) -> TypeRef {
        match node {
            hir::Type::Named(ref mut n) => {
                self.define_type_name(n, RefKind::Default)
            }
            hir::Type::Ref(_) | hir::Type::Mut(_) if !self.rules.allow_refs => {
                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    "references to types aren't allowed here",
                    self.file(),
                    node.location().clone(),
                );
                TypeRef::Error
            }
            hir::Type::Ref(ref mut n) => {
                self.define_reference_type(n, RefKind::Ref)
            }
            hir::Type::Mut(ref mut n) => {
                self.define_reference_type(n, RefKind::Mut)
            }
            hir::Type::Uni(ref mut n) => {
                self.define_reference_type(n, RefKind::Uni)
            }
            hir::Type::Owned(ref mut n) => {
                self.define_reference_type(n, RefKind::Owned)
            }
            hir::Type::Closure(ref mut n) => {
                self.define_closure_type(n, RefKind::Owned)
            }
            hir::Type::Tuple(ref mut n) => {
                self.define_tuple_type(n, RefKind::Owned)
            }
        }
    }

    fn define_reference_type(
        &mut self,
        node: &mut hir::ReferenceType,
        kind: RefKind,
    ) -> TypeRef {
        match node.type_reference {
            hir::ReferrableType::Named(ref mut n) => {
                self.define_type_name(n, kind)
            }
            hir::ReferrableType::Closure(ref mut n) => {
                self.define_closure_type(n, kind)
            }
            hir::ReferrableType::Tuple(ref mut n) => {
                self.define_tuple_type(n, kind)
            }
        }
    }

    fn define_type_name(
        &mut self,
        node: &mut hir::TypeName,
        kind: RefKind,
    ) -> TypeRef {
        let name = &node.name.name;
        let symbol = if let Some(source) = node.source.as_ref() {
            if let Some(Symbol::Module(module)) =
                self.scope.symbol(self.db(), &source.name)
            {
                module.symbol(self.db(), name)
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!("the symbol '{}' isn't a module", source.name),
                    self.file(),
                    source.location.clone(),
                );

                return TypeRef::Error;
            }
        } else {
            self.scope.symbol(self.db(), name)
        };

        node.resolved_type = if let Some(symbol) = symbol {
            if !self.rules.allow_private_types && symbol.is_private(self.db()) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "'{}' is private, but private types can't be used here",
                        name
                    ),
                    self.file(),
                    node.name.location.clone(),
                );

                return TypeRef::Error;
            }

            match symbol {
                Symbol::Class(id) if id.kind(&self.state.db).is_extern() => {
                    TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(
                        id,
                    )))
                }
                Symbol::Class(id) => {
                    kind.into_type_ref(self.define_class_instance(id, node))
                }
                Symbol::Trait(id) => {
                    kind.into_type_ref(self.define_trait_instance(id, node))
                }
                Symbol::TypeParameter(id) => {
                    self.define_type_parameter(id, node, kind)
                }
                _ => {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidType,
                        format!("'{}' isn't a type", name),
                        self.file(),
                        node.name.location.clone(),
                    );

                    return TypeRef::Error;
                }
            }
        } else {
            // We assume special types such as Never are used less often
            // compared to physical types, so we handle them here rather than
            // handling them first.
            match name.as_str() {
                "Never" => {
                    if kind == RefKind::Default {
                        TypeRef::Never
                    } else {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidType,
                            "'Never' can't be used as a reference",
                            self.file(),
                            node.location.clone(),
                        );

                        return TypeRef::Error;
                    }
                }
                name => {
                    if let Some(ctype) = self.resolve_foreign_type(
                        name,
                        &node.arguments,
                        &node.location,
                    ) {
                        ctype
                    } else {
                        TypeRef::Error
                    }
                }
            }
        };

        node.resolved_type
    }

    fn define_tuple_type(
        &mut self,
        node: &mut hir::TupleType,
        kind: RefKind,
    ) -> TypeRef {
        let class = if let Some(id) = ClassId::tuple(node.values.len()) {
            id
        } else {
            self.state
                .diagnostics
                .tuple_size_error(self.file(), node.location.clone());

            return TypeRef::Error;
        };

        let types =
            node.values.iter_mut().map(|n| self.define_type(n)).collect();
        let ins = TypeId::ClassInstance(ClassInstance::with_types(
            self.db_mut(),
            class,
            types,
        ));

        kind.into_type_ref(ins)
    }

    fn define_class_instance(
        &mut self,
        id: ClassId,
        node: &mut hir::TypeName,
    ) -> TypeId {
        let params = id.type_parameters(self.db());

        if let Some(args) = self.type_arguments(params, &mut node.arguments) {
            TypeId::ClassInstance(ClassInstance::generic(
                self.db_mut(),
                id,
                args,
            ))
        } else {
            TypeId::ClassInstance(ClassInstance::new(id))
        }
    }

    fn define_trait_instance(
        &mut self,
        id: TraitId,
        node: &mut hir::TypeName,
    ) -> TypeId {
        let params = id.type_parameters(self.db());

        if let Some(args) = self.type_arguments(params, &mut node.arguments) {
            TypeId::TraitInstance(TraitInstance::generic(
                self.db_mut(),
                id,
                args,
            ))
        } else {
            TypeId::TraitInstance(TraitInstance::new(id))
        }
    }

    fn define_type_parameter(
        &mut self,
        id: TypeParameterId,
        node: &hir::TypeName,
        kind: RefKind,
    ) -> TypeRef {
        if !node.arguments.is_empty() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                "type parameters don't support type arguments",
                self.file(),
                node.location.clone(),
            );
        }

        let param_id =
            self.scope.bounds.as_ref().and_then(|b| b.get(id)).unwrap_or(id);

        let type_id = if self.rules.type_parameters_as_rigid {
            TypeId::RigidTypeParameter(param_id)
        } else {
            TypeId::TypeParameter(param_id)
        };

        if let RefKind::Mut = kind {
            if !param_id.is_mutable(self.db()) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "the type 'mut {name}' is invalid, as '{name}' \
                            might be immutable at runtime",
                        name = id.name(self.db()),
                    ),
                    self.file(),
                    node.location.clone(),
                );
            }
        }

        kind.into_type_ref(type_id)
    }

    fn define_closure_type(
        &mut self,
        node: &mut hir::ClosureType,
        kind: RefKind,
    ) -> TypeRef {
        let block = Closure::alloc(self.db_mut(), false);

        for arg_node in &mut node.arguments {
            let typ = self.define_type(arg_node);

            block.new_anonymous_argument(self.db_mut(), typ);
        }

        let return_type = if let Some(type_node) = node.return_type.as_mut() {
            self.define_type(type_node)
        } else {
            TypeRef::nil()
        };

        block.set_return_type(self.db_mut(), return_type);

        let typ = kind.into_type_ref(TypeId::Closure(block));

        node.resolved_type = typ;
        typ
    }

    fn type_arguments(
        &mut self,
        parameters: Vec<TypeParameterId>,
        arguments: &mut [hir::Type],
    ) -> Option<TypeArguments> {
        if parameters.is_empty() {
            return None;
        }

        let mut targs = TypeArguments::new();

        for (arg_node, param) in arguments.iter_mut().zip(parameters) {
            targs.assign(param, self.define_type(arg_node));
        }

        Some(targs)
    }

    fn resolve_foreign_type(
        &mut self,
        name: &str,
        arguments: &[hir::Type],
        location: &SourceLocation,
    ) -> Option<TypeRef> {
        match name {
            "Int8" => Some(TypeRef::foreign_signed_int(8)),
            "Int16" => Some(TypeRef::foreign_signed_int(16)),
            "Int32" => Some(TypeRef::foreign_signed_int(32)),
            "Int64" => Some(TypeRef::foreign_signed_int(64)),
            "UInt8" => Some(TypeRef::foreign_unsigned_int(8)),
            "UInt16" => Some(TypeRef::foreign_unsigned_int(16)),
            "UInt32" => Some(TypeRef::foreign_unsigned_int(32)),
            "UInt64" => Some(TypeRef::foreign_unsigned_int(64)),
            "Float32" => Some(TypeRef::foreign_float(32)),
            "Float64" => Some(TypeRef::foreign_float(64)),
            "Pointer" => {
                if arguments.len() != 1 {
                    self.state.diagnostics.incorrect_number_of_type_arguments(
                        1,
                        arguments.len(),
                        self.file(),
                        location.clone(),
                    );

                    return None;
                }

                let arg = if let hir::Type::Named(n) = &arguments[0] {
                    self.resolve_foreign_type(
                        &n.name.name,
                        &n.arguments,
                        &n.location,
                    )
                } else {
                    None
                }?;

                match arg {
                    TypeRef::Owned(v) => Some(TypeRef::Pointer(v)),
                    TypeRef::Pointer(_) => {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidType,
                            "nested pointers (e.g. 'Pointer[Pointer[UInt8]]') \
                            aren't supported, you should use regular \
                            pointers instead",
                            self.file(),
                            location.clone(),
                        );

                        None
                    }
                    _ => Some(arg),
                }
            }
            name => match self.scope.symbol(self.db(), name) {
                Some(Symbol::Class(id)) => Some(TypeRef::Owned(
                    TypeId::ClassInstance(ClassInstance::new(id)),
                )),
                Some(Symbol::TypeParameter(id)) => {
                    let tid = if self.rules.type_parameters_as_rigid {
                        TypeId::RigidTypeParameter(id)
                    } else {
                        TypeId::TypeParameter(id)
                    };

                    Some(TypeRef::Owned(tid))
                }
                Some(_) => {
                    self.state.diagnostics.invalid_c_type(
                        name,
                        self.file(),
                        location.clone(),
                    );

                    None
                }
                _ => {
                    self.state.diagnostics.undefined_symbol(
                        name,
                        self.file(),
                        location.clone(),
                    );

                    None
                }
            },
        }
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }
}

/// A visitor that checks if a type in a type signature is valid.
///
/// The type `DefineTypeSignature` is tasked with _just_ defining a type,
/// without validating (for example) the number of type arguments, and if type
/// arguments can be assigned to their corresponding type parameters. This
/// `CheckType` type is tasked with doing just that. By splitting this up we can
/// allow for circular types, such as the following example:
///
///     trait A[T: B[Int]] {}
///     trait B[T: A[Int]] {}
///
/// If type defining and checking took place in a single iteration/pass this
/// would't work, as we wouldn't be able to accurately define `B[Int]` before
/// processing `A[Int]`, which in turn we can't process before processing
/// `B[Int]`.
///
/// In addition, if defining and checking took place in the same pass, we
/// wouldn't be able to support code like this:
///
///     class Int {}
///
///     trait A[T: ToString]
///     trait B[T: A[Int]]
///
///     impl ToString for Int { ... }
///
/// Supporting this requires that we first define the type parameters and their
/// requirements, then define all trait implementations, _then_ check
/// the requirements and implementations.
pub(crate) struct CheckTypeSignature<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> CheckTypeSignature<'a> {
    pub(crate) fn new(state: &'a mut State, module: ModuleId) -> Self {
        Self { state, module }
    }

    pub(crate) fn check(&mut self, node: &hir::Type) {
        match node {
            hir::Type::Named(ref n) => self.check_type_name(n),
            hir::Type::Ref(ref n) => self.check_reference_type(n),
            hir::Type::Uni(ref n) => self.check_reference_type(n),
            hir::Type::Mut(ref n) => self.check_reference_type(n),
            hir::Type::Owned(ref n) => self.check_reference_type(n),
            hir::Type::Closure(ref n) => self.check_closure_type(n),
            hir::Type::Tuple(ref n) => self.check_tuple_type(n),
        }
    }

    pub(crate) fn check_type_name(&mut self, node: &hir::TypeName) {
        match node.resolved_type {
            TypeRef::Owned(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::Uni(id) => match id {
                TypeId::ClassInstance(ins) => {
                    self.check_class_instance(node, ins);
                }
                TypeId::TraitInstance(ins) => {
                    self.check_trait_instance(node, ins);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn check_reference_type(&mut self, node: &hir::ReferenceType) {
        match node.type_reference {
            hir::ReferrableType::Named(ref n) => self.check_type_name(n),
            hir::ReferrableType::Closure(ref n) => self.check_closure_type(n),
            hir::ReferrableType::Tuple(ref n) => self.check_tuple_type(n),
        }
    }

    fn check_class_instance(
        &mut self,
        node: &hir::TypeName,
        instance: ClassInstance,
    ) {
        let required =
            instance.instance_of().number_of_type_parameters(self.db());

        if self.check_type_argument_count(node, required) {
            // Classes can't allow Any types as type arguments, as this results
            // in a loss of type information at runtime. This means that if a
            // class stores a type parameter T in a field, and it's assigned to
            // Any, we have no idea how to drop that value, and the value might
            // not even be managed by Inko (e.g. when using the FFI).
            self.check_argument_types(
                node,
                instance.instance_of().type_parameters(self.db()),
                instance.type_arguments(self.db()).clone(),
            );
        }
    }

    fn check_trait_instance(
        &mut self,
        node: &hir::TypeName,
        instance: TraitInstance,
    ) {
        let required =
            instance.instance_of().number_of_type_parameters(self.db());

        if self.check_type_argument_count(node, required) {
            // Traits do allow Any types as type arguments, as traits don't
            // dictate how a value is stored. If we end up dropping a trait we
            // do so by calling the dropper of the underlying class, which in
            // turn already disallows storing Any in generic contexts.
            self.check_argument_types(
                node,
                instance.instance_of().type_parameters(self.db()),
                instance.type_arguments(self.db()).clone(),
            );
        }
    }

    fn check_type_argument_count(
        &mut self,
        node: &hir::TypeName,
        required: usize,
    ) -> bool {
        let given = node.arguments.len();

        if given == 0 && required == 0 {
            return false;
        }

        if given != required {
            self.state.diagnostics.incorrect_number_of_type_arguments(
                required,
                given,
                self.file(),
                node.location.clone(),
            );

            return false;
        }

        true
    }

    fn check_argument_types(
        &mut self,
        node: &hir::TypeName,
        parameters: Vec<TypeParameterId>,
        arguments: TypeArguments,
    ) {
        let exp_args =
            parameters.iter().fold(TypeArguments::new(), |mut args, &p| {
                args.assign(p, TypeRef::placeholder(self.db_mut(), Some(p)));
                args
            });

        for (param, node) in parameters.into_iter().zip(node.arguments.iter()) {
            let arg = arguments.get(param).unwrap();
            let exp = TypeRef::Any(TypeId::TypeParameter(param));
            let mut env = Environment::new(
                arg.type_arguments(self.db()),
                exp_args.clone(),
            );

            if !TypeChecker::new(self.db()).run(arg, exp, &mut env) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "'{}' can't be assigned to type parameter '{}'",
                        format_type(self.db(), arg),
                        format_type(self.db(), param)
                    ),
                    self.file(),
                    node.location().clone(),
                );
            }

            self.check(node);
        }
    }

    fn check_closure_type(&mut self, node: &hir::ClosureType) {
        for node in &node.arguments {
            self.check(node);
        }

        if let Some(node) = node.return_type.as_ref() {
            self.check(node);
        }
    }

    fn check_tuple_type(&mut self, node: &hir::TupleType) {
        for node in &node.values {
            self.check(node);
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

/// A visitor that combines `DefineTypeSignature` and `CheckTypeSignature`.
pub(crate) struct DefineAndCheckTypeSignature<'a> {
    state: &'a mut State,
    module: ModuleId,
    scope: &'a TypeScope<'a>,
    rules: Rules,
}

impl<'a> DefineAndCheckTypeSignature<'a> {
    pub(crate) fn new(
        state: &'a mut State,
        module: ModuleId,
        scope: &'a TypeScope<'a>,
        rules: Rules,
    ) -> Self {
        Self { state, module, scope, rules }
    }

    pub(crate) fn define_type(&mut self, node: &mut hir::Type) -> TypeRef {
        let typ = DefineTypeSignature::new(
            self.state,
            self.module,
            self.scope,
            self.rules,
        )
        .define_type(node);

        CheckTypeSignature::new(self.state, self.module).check(node);

        typ
    }

    pub(crate) fn as_trait_instance(
        &mut self,
        node: &mut hir::TypeName,
    ) -> Option<TraitInstance> {
        let ins = DefineTypeSignature::new(
            self.state,
            self.module,
            self.scope,
            self.rules,
        )
        .as_trait_instance(node);

        if ins.is_some() {
            CheckTypeSignature::new(self.state, self.module)
                .check_type_name(node);
        }

        ins
    }
}

pub(crate) fn define_type_bounds(
    state: &mut State,
    module: ModuleId,
    class: ClassId,
    nodes: &mut [hir::TypeBound],
) -> TypeBounds {
    let mut bounds = TypeBounds::new();

    for bound in nodes {
        let name = &bound.name.name;
        let param = if let Some(id) = class.type_parameter(&state.db, name) {
            id
        } else {
            state.diagnostics.undefined_symbol(
                name,
                module.file(&state.db),
                bound.name.location.clone(),
            );

            continue;
        };

        if bounds.get(param).is_some() {
            state.diagnostics.error(
                DiagnosticId::DuplicateSymbol,
                format!(
                    "bounds are already defined for type parameter '{}'",
                    name
                ),
                module.file(&state.db),
                bound.location.clone(),
            );

            continue;
        }

        let mut reqs = param.requirements(&state.db);
        let new_param = TypeParameter::alloc(&mut state.db, name.clone());

        for req in &mut bound.requirements {
            let rules = Rules::default();
            let scope = TypeScope::new(module, TypeId::Class(class), None);
            let mut definer =
                DefineTypeSignature::new(state, module, &scope, rules);

            if let Some(ins) = definer.as_trait_instance(req) {
                reqs.push(ins);
            }
        }

        if bound.mutable {
            new_param.set_mutable(&mut state.db);
        }

        new_param.set_original(&mut state.db, param);
        new_param.add_requirements(&mut state.db, reqs);
        bounds.set(param, new_param);
    }

    bounds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnostics::DiagnosticId;
    use crate::hir;
    use crate::test::{cols, hir_type_name, module_type};
    use crate::type_check::{DefineTypeSignature, TypeScope};
    use types::{
        Class, ClassKind, ClosureId, Location, Method, MethodKind, Trait,
        TypeId, TypeRef, Visibility,
    };

    macro_rules! variant {
        ($enum: expr, $pattern: path) => {{
            if let $pattern(ref node) = $enum {
                node
            } else {
                panic!("unexpected enum variant")
            }
        }};
    }

    #[test]
    fn test_type_scope_new() {
        let mut state = State::new(Config::new());
        let int = Class::alloc(
            &mut state.db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let module = module_type(&mut state, "foo");
        let scope = TypeScope::new(module, self_type, None);

        assert_eq!(scope.module, module);
        assert_eq!(scope.self_type, self_type);
        assert!(scope.method.is_none());
    }

    #[test]
    fn test_type_scope_symbol() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let method = Method::alloc(
            &mut state.db,
            module,
            Location::new(1..=1, 1..=1),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let array = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        let method_param =
            method.new_type_parameter(&mut state.db, "A".to_string());
        let self_param =
            array.new_type_parameter(&mut state.db, "B".to_string());

        module.new_symbol(
            &mut state.db,
            "Array".to_string(),
            Symbol::Class(array),
        );

        let array_ins = TypeId::ClassInstance(ClassInstance::new(array));
        let scope = TypeScope::new(module, array_ins, Some(method));

        assert_eq!(
            scope.symbol(&state.db, "A"),
            Some(Symbol::TypeParameter(method_param))
        );
        assert_eq!(
            scope.symbol(&state.db, "B"),
            Some(Symbol::TypeParameter(self_param))
        );
        assert_eq!(
            scope.symbol(&state.db, "Array"),
            Some(Symbol::Class(array))
        );
        assert!(scope.symbol(&state.db, "Foo").is_none());
    }

    #[test]
    fn test_define_type_signature_as_trait_instance_with_trait() {
        let mut state = State::new(Config::new());
        let int = Class::alloc(
            &mut state.db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let module = module_type(&mut state, "foo");
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

        let to_string_ins = TraitInstance::new(to_string);
        let scope = TypeScope::new(module, self_type, None);
        let mut node = hir_type_name("ToString", Vec::new(), cols(1, 1));
        let rules = Rules::default();
        let typ = DefineTypeSignature::new(&mut state, module, &scope, rules)
            .as_trait_instance(&mut node);

        assert_eq!(typ, Some(to_string_ins));
        assert!(!state.diagnostics.has_errors());
    }

    #[test]
    fn test_define_type_signature_as_trait_instance_with_invalid_type() {
        let mut state = State::new(Config::new());
        let int = Class::alloc(
            &mut state.db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let module = module_type(&mut state, "foo");
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
            "String".to_string(),
            Symbol::Class(string),
        );

        let scope = TypeScope::new(module, self_type, None);
        let mut node = hir_type_name("String", Vec::new(), cols(1, 1));
        let rules = Rules::default();
        let typ = DefineTypeSignature::new(&mut state, module, &scope, rules)
            .as_trait_instance(&mut node);

        assert!(typ.is_none());
        assert!(state.diagnostics.has_errors());
    }

    #[test]
    fn test_define_type_signature_with_owned_type() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let class_id = Class::alloc(
            &mut state.db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let class_instance =
            TypeId::ClassInstance(ClassInstance::new(class_id));

        module.new_symbol(
            &mut state.db,
            "A".to_string(),
            Symbol::Class(class_id),
        );

        let mut node = hir::Type::Named(Box::new(hir_type_name(
            "A",
            Vec::new(),
            cols(1, 1),
        )));
        let scope = TypeScope::new(module, class_instance, None);
        let rules = Rules::default();
        let type_ref =
            DefineTypeSignature::new(&mut state, module, &scope, rules)
                .define_type(&mut node);

        assert!(!state.diagnostics.has_errors());
        assert_eq!(type_ref, TypeRef::Owned(class_instance));
        assert_eq!(variant!(node, hir::Type::Named).resolved_type, type_ref);
    }

    #[test]
    fn test_define_type_signature_with_namespaced_type() {
        let mut state = State::new(Config::new());
        let foo_mod = module_type(&mut state, "foo");
        let bar_mod = module_type(&mut state, "bar");
        let class_id = Class::alloc(
            &mut state.db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let class_instance =
            TypeId::ClassInstance(ClassInstance::new(class_id));

        foo_mod.new_symbol(
            &mut state.db,
            "A".to_string(),
            Symbol::Class(class_id),
        );

        bar_mod.new_symbol(
            &mut state.db,
            "foo".to_string(),
            Symbol::Module(foo_mod),
        );

        let mut node = hir::Type::Named(Box::new(hir::TypeName {
            source: Some(hir::Identifier {
                name: "foo".to_string(),
                location: cols(1, 1),
            }),
            resolved_type: TypeRef::Unknown,
            name: hir::Constant { name: "A".to_string(), location: cols(1, 1) },
            arguments: Vec::new(),
            location: cols(1, 1),
        }));

        let scope = TypeScope::new(bar_mod, class_instance, None);
        let rules = Rules::default();
        let type_ref =
            DefineTypeSignature::new(&mut state, bar_mod, &scope, rules)
                .define_type(&mut node);

        assert!(!state.diagnostics.has_errors());
        assert_eq!(type_ref, TypeRef::Owned(class_instance));
        assert_eq!(variant!(node, hir::Type::Named).resolved_type, type_ref);
    }

    #[test]
    fn test_define_type_signature_with_private_type() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let class_id = Class::alloc(
            &mut state.db,
            "_A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let class_instance =
            TypeId::ClassInstance(ClassInstance::new(class_id));

        module.new_symbol(
            &mut state.db,
            "_A".to_string(),
            Symbol::Class(class_id),
        );

        let mut node = hir::Type::Named(Box::new(hir_type_name(
            "_A",
            Vec::new(),
            cols(1, 1),
        )));
        let scope = TypeScope::new(module, class_instance, None);
        let rules = Rules { allow_private_types: false, ..Default::default() };

        DefineTypeSignature::new(&mut state, module, &scope, rules)
            .define_type(&mut node);

        assert!(state.diagnostics.has_errors());

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
    }

    #[test]
    fn test_define_type_signature_with_ref_type() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let class_id = Class::alloc(
            &mut state.db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let class_instance =
            TypeId::ClassInstance(ClassInstance::new(class_id));

        module.new_symbol(
            &mut state.db,
            "A".to_string(),
            Symbol::Class(class_id),
        );

        let mut node = hir::Type::Ref(Box::new(hir::ReferenceType {
            type_reference: hir::ReferrableType::Named(Box::new(
                hir_type_name("A", Vec::new(), cols(1, 1)),
            )),
            location: cols(1, 1),
        }));
        let scope = TypeScope::new(module, class_instance, None);
        let rules = Rules::default();
        let type_ref =
            DefineTypeSignature::new(&mut state, module, &scope, rules)
                .define_type(&mut node);

        assert!(!state.diagnostics.has_errors());
        assert_eq!(type_ref, TypeRef::Ref(class_instance));

        assert_eq!(
            variant!(
                variant!(node, hir::Type::Ref).type_reference,
                hir::ReferrableType::Named
            )
            .resolved_type,
            type_ref
        );
    }

    #[test]
    fn test_define_type_signature_with_closure_type() {
        let mut state = State::new(Config::new());
        let int = Class::alloc(
            &mut state.db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let module = module_type(&mut state, "foo");
        let mut node = hir::Type::Closure(Box::new(hir::ClosureType {
            arguments: Vec::new(),
            return_type: None,
            location: cols(1, 1),
            resolved_type: TypeRef::Unknown,
        }));
        let scope = TypeScope::new(module, self_type, None);
        let rules = Rules::default();
        let type_ref =
            DefineTypeSignature::new(&mut state, module, &scope, rules)
                .define_type(&mut node);

        assert_eq!(type_ref, TypeRef::Owned(TypeId::Closure(ClosureId(0))));
        assert!(!state.diagnostics.has_errors());
        assert_eq!(variant!(node, hir::Type::Closure).resolved_type, type_ref);
    }

    #[test]
    fn test_check_type_signature_with_incorrect_number_of_arguments() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let class_a = Class::alloc(
            &mut state.db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let class_b = Class::alloc(
            &mut state.db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let instance_a = TypeId::ClassInstance(ClassInstance::new(class_a));

        module.new_symbol(
            &mut state.db,
            "A".to_string(),
            Symbol::Class(class_a),
        );

        module.new_symbol(
            &mut state.db,
            "B".to_string(),
            Symbol::Class(class_b),
        );

        let mut node = hir::Type::Named(Box::new(hir_type_name(
            "A",
            vec![hir::Type::Named(Box::new(hir_type_name(
                "B",
                Vec::new(),
                cols(2, 2),
            )))],
            cols(1, 1),
        )));

        let scope = TypeScope::new(module, instance_a, None);
        let rules = Rules::default();

        DefineTypeSignature::new(&mut state, module, &scope, rules)
            .define_type(&mut node);

        CheckTypeSignature::new(&mut state, module).check(&node);

        assert!(state.diagnostics.has_errors());

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidType);
        assert_eq!(error.location(), &cols(1, 1));
    }

    #[test]
    fn test_check_type_signature_with_incompatible_type_arguments() {
        let mut state = State::new(Config::new());
        let module = module_type(&mut state, "foo");
        let to_string = Trait::alloc(
            &mut state.db,
            "ToString".to_string(),
            Visibility::Private,
            module,
            Location::default(),
        );
        let list_class = Class::alloc(
            &mut state.db,
            "List".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let list_param =
            list_class.new_type_parameter(&mut state.db, "T".to_string());
        let requirement = TraitInstance::new(to_string);

        list_param.add_requirements(&mut state.db, vec![requirement]);

        let string_class = Class::alloc(
            &mut state.db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let instance_a = TypeId::ClassInstance(ClassInstance::rigid(
            &mut state.db,
            list_class,
            &TypeBounds::new(),
        ));

        module.new_symbol(
            &mut state.db,
            "List".to_string(),
            Symbol::Class(list_class),
        );

        module.new_symbol(
            &mut state.db,
            "String".to_string(),
            Symbol::Class(string_class),
        );

        module.new_symbol(
            &mut state.db,
            "ToString".to_string(),
            Symbol::Trait(to_string),
        );

        let mut node = hir::Type::Named(Box::new(hir_type_name(
            "List",
            vec![hir::Type::Named(Box::new(hir_type_name(
                "String",
                Vec::new(),
                cols(2, 2),
            )))],
            cols(1, 1),
        )));

        let scope = TypeScope::new(module, instance_a, None);
        let rules = Rules::default();

        DefineTypeSignature::new(&mut state, module, &scope, rules)
            .define_type(&mut node);

        CheckTypeSignature::new(&mut state, module).check(&node);

        assert!(state.diagnostics.has_errors());

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidType);
        assert_eq!(error.location(), &cols(2, 2));
    }
}
