//! Passes for type-checking method body and constant expressions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{DefineAndCheckTypeSignature, Rules, TypeScope};
use ast::source_location::SourceLocation;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use types::check::{Environment, TypeChecker};
use types::format::{format_type, format_type_with_arguments};
use types::resolve::TypeResolver;
use types::{
    Block, BuiltinCallInfo, CallInfo, CallKind, ClassId, ClassInstance,
    Closure, ClosureCallInfo, ClosureId, ConstantKind, ConstantPatternKind,
    Database, FieldId, FieldInfo, IdentifierKind, MethodId, MethodKind,
    MethodLookup, MethodSource, ModuleId, Receiver, Symbol, ThrowKind, TraitId,
    TraitInstance, TypeArguments, TypeBounds, TypeId, TypeRef, Variable,
    VariableId, CALL_METHOD,
};

const IGNORE_VARIABLE: &str = "_";
const STRING_LITERAL_LIMIT: usize = u32::MAX as usize;
const CONST_ARRAY_LIMIT: usize = u16::MAX as usize;

/// The maximum number of methods that a single class can define.
///
/// We subtract 1 to account for the generated dropper methods, as these methods
/// are generated later.
const METHODS_IN_CLASS_LIMIT: usize = (u16::MAX - 1) as usize;

struct Pattern<'a> {
    /// The variable scope to use for defining variables introduced by patterns.
    variable_scope: &'a mut VariableScope,

    /// The variables introduced by this pattern.
    variables: HashMap<String, VariableId>,
}

impl<'a> Pattern<'a> {
    fn new(variable_scope: &'a mut VariableScope) -> Self {
        Self { variable_scope, variables: HashMap::new() }
    }
}

/// A collection of variables defined in a lexical scope.
struct VariableScope {
    /// The variables defined in this scope.
    variables: HashMap<String, VariableId>,
}

impl VariableScope {
    fn new() -> Self {
        Self { variables: HashMap::new() }
    }

    fn new_variable(
        &mut self,
        db: &mut Database,
        name: String,
        value_type: TypeRef,
        mutable: bool,
    ) -> VariableId {
        let var = Variable::alloc(db, name.clone(), value_type, mutable);

        self.add_variable(name, var);
        var
    }

    fn add_variable(&mut self, name: String, variable: VariableId) {
        self.variables.insert(name, variable);
    }

    fn variable(&self, name: &str) -> Option<VariableId> {
        self.variables.get(name).cloned()
    }

    fn names(&self) -> Vec<&String> {
        self.variables.keys().collect()
    }
}

#[derive(Eq, PartialEq)]
enum ScopeKind {
    Closure(ClosureId),
    Loop,
    Method,
    Regular,
    Recover,
}

struct LexicalScope<'a> {
    kind: ScopeKind,

    /// The return type of the surrounding block.
    return_type: TypeRef,

    /// The type of `self` in this scope.
    ///
    /// The type of `self` may change based on the context it's used in. For
    /// example, in a moving method the type is `T`, but in a closure that
    /// captures it the type would be `mut T`.
    ///
    /// Instead of calculating the correct type every time we need it, we
    /// calculate it once per scope.
    surrounding_type: TypeRef,

    /// The variables defined in this scope.
    variables: VariableScope,

    /// The parent of this scope.
    parent: Option<&'a LexicalScope<'a>>,

    /// A boolean indicating that we're in a closure.
    ///
    /// This flag allows us to quickly check if we're in a closure, without
    /// having to walk the scope up every time.
    in_closure: bool,

    /// A boolean indicating that we broke out of this loop scope using `break`.
    ///
    /// We use a Cell here as each scope's parent is an immutable reference, as
    /// using mutable references leads to all sorts of borrowing issues.
    break_in_loop: Cell<bool>,
}

impl<'a> LexicalScope<'a> {
    fn method(self_type: TypeRef, return_type: TypeRef) -> Self {
        Self {
            kind: ScopeKind::Method,
            variables: VariableScope::new(),
            surrounding_type: self_type,
            return_type,
            parent: None,
            in_closure: false,
            break_in_loop: Cell::new(false),
        }
    }

    fn inherit(&'a self, kind: ScopeKind) -> Self {
        Self {
            kind,
            surrounding_type: self.surrounding_type,
            return_type: self.return_type,
            variables: VariableScope::new(),
            parent: Some(self),
            in_closure: self.in_closure,
            break_in_loop: Cell::new(false),
        }
    }

    fn in_loop(&self) -> bool {
        self.inside(ScopeKind::Loop)
    }

    fn in_recover(&self) -> bool {
        self.inside(ScopeKind::Recover)
    }

    fn in_closure_in_recover(&self) -> bool {
        if !self.in_closure {
            return false;
        }

        let mut scope = Some(self);
        let mut in_closure = false;

        while let Some(current) = scope {
            match current.kind {
                ScopeKind::Closure(_) => in_closure = true,
                ScopeKind::Recover if in_closure => return true,
                _ => {}
            }

            scope = current.parent;
        }

        false
    }

    fn mark_closures_as_capturing_self(&self, db: &mut Database) {
        if !self.in_closure {
            return;
        }

        let mut scope = Some(self);

        while let Some(current) = scope {
            if let ScopeKind::Closure(id) = current.kind {
                if let Some(parent) = current.parent {
                    id.set_captured_self_type(db, parent.surrounding_type);
                }
            }

            scope = current.parent;
        }
    }

    fn inside(&self, kind: ScopeKind) -> bool {
        let mut scope = Some(self);

        while let Some(current) = scope {
            if current.kind == kind {
                return true;
            }

            scope = current.parent;
        }

        false
    }
}

struct MethodCall {
    /// The module the method call resides in.
    module: ModuleId,

    /// The method that's called.
    method: MethodId,

    /// The base type arguments to use for type-checking.
    type_arguments: TypeArguments,

    /// A union of the type bounds of the surrounding and the called method.
    ///
    /// These bounds are to be used when inferring types, such as the return
    /// type.
    bounds: TypeBounds,

    /// The type of the method's receiver.
    receiver: TypeRef,

    /// The number of arguments specified.
    arguments: usize,

    /// The named arguments that have been specified thus far.
    named_arguments: HashSet<String>,

    /// If input/output types should be limited to sendable types.
    require_sendable: bool,

    /// Arguments of which we need to check if they are sendable.
    check_sendable: Vec<(TypeRef, SourceLocation)>,

    /// The resolved return type of the call.
    return_type: TypeRef,
}

impl MethodCall {
    fn new(
        state: &mut State,
        module: ModuleId,
        surrounding_scope: Option<(TypeId, MethodId)>,
        receiver: TypeRef,
        receiver_id: TypeId,
        method: MethodId,
    ) -> Self {
        // When checking arguments we need access to the type arguments of the
        // receiver, along with any type arguments introduced by the method
        // itself.
        let mut type_arguments = receiver.type_arguments(&state.db);

        // Type parameters may be reused between arguments and throw/return
        // types, so we need to ensure all references resolve into the same
        // types, hence we create type placeholders here.
        for param in method.type_parameters(&state.db).into_iter() {
            type_arguments.assign(
                param,
                TypeRef::placeholder(&mut state.db, Some(param)),
            );
        }

        // Static methods may use/return type parameters of the surrounding
        // type, so we also need to create placeholders for those.
        if method.kind(&state.db) == MethodKind::Static {
            if let TypeId::Class(class) = receiver_id {
                if class.is_generic(&state.db) {
                    for param in class.type_parameters(&state.db) {
                        type_arguments.assign(
                            param,
                            TypeRef::placeholder(&mut state.db, Some(param)),
                        );
                    }
                }
            }
        }

        // When a method is implemented through a trait, it may depend on type
        // parameters of that trait. To ensure these are mapped to the final
        // inferred types, we have to copy them over into our temporary type
        // arguments. To illustrate:
        //
        //     trait Example[R] {
        //       fn example -> R {}
        //     }
        //
        //     class List[T] {}
        //
        //     impl Example[T] for List {}
        //
        // When we call `example` on a `List[Int]`, the return type should be
        // `Int` and not `R`. For a `List[Int]`, its own type arguments only
        // consider its own type parameters (`T -> Int` in this case), not those
        // of any traits that may be implemented.
        //
        // Copying the mapping of the implementation that introduced a method
        // makes it possible to produce the right type: when we find `R` we see
        // it's assigned to `T` (because of `impl Example[T] ...`), we then map
        // that to its assigned value (`Int`), and we're good to go.
        //
        // We only need to do this when the receiver is a class (and thus the
        // method has a source). If the receiver is a trait, we'll end up using
        // its inherited type arguments when inferring a type parameter.
        if let MethodSource::Implementation(ins, _) = method.source(&state.db) {
            ins.copy_type_arguments_into(&state.db, &mut type_arguments);
        }

        let require_sendable = receiver.require_sendable_arguments(&state.db)
            && !method.is_moving(&state.db);

        let bounds = if let Some((self_id, self_method)) = surrounding_scope {
            // When calling a method on `self`, we need to take any surrounding
            // bounds into account when resolving types.
            if self_id == receiver_id {
                self_method.bounds(&state.db).union(method.bounds(&state.db))
            } else {
                self_method.bounds(&state.db).clone()
            }
        } else {
            TypeBounds::new()
        };

        Self {
            module,
            method,
            bounds,
            receiver,
            type_arguments,
            arguments: 0,
            named_arguments: HashSet::new(),
            require_sendable,
            check_sendable: Vec::new(),
            return_type: TypeRef::Unknown,
        }
    }

    fn check_type_bounds(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) {
        let bounds = self.method.bounds(&state.db);
        let args = self.type_arguments.clone();
        let mut scope = Environment::new(args.clone(), args);
        let mut checker = TypeChecker::new(&state.db);

        if !checker.check_bounds(bounds, &mut scope) {
            state.diagnostics.error(
                DiagnosticId::InvalidSymbol,
                format!(
                    "The method '{}' exists but isn't available because \
                    one or more type parameter bounds aren't met",
                    self.method.name(&state.db),
                ),
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn check_arguments(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) {
        let expected = self.method.number_of_arguments(&state.db);

        if self.arguments != expected {
            state.diagnostics.incorrect_call_arguments(
                self.arguments,
                expected,
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn check_mutability(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) {
        let name = self.method.name(&state.db);
        let rec = self.receiver;

        if self.method.is_moving(&state.db) && !rec.allow_moving() {
            state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "The method '{}' takes ownership of its receiver, \
                    but '{}' isn't an owned value",
                    name,
                    format_type_with_arguments(
                        &state.db,
                        &self.type_arguments,
                        rec
                    )
                ),
                self.module.file(&state.db),
                location.clone(),
            );

            return;
        }

        if self.method.is_mutable(&state.db) && !rec.allow_mutating() {
            state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "The method '{}' requires a mutable receiver, \
                    but '{}' isn't mutable",
                    name,
                    format_type_with_arguments(
                        &state.db,
                        &self.type_arguments,
                        rec
                    )
                ),
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn check_argument(
        &mut self,
        state: &mut State,
        argument: TypeRef,
        expected: TypeRef,
        location: &SourceLocation,
    ) {
        let given = argument.cast_according_to(expected, &state.db);

        if self.require_sendable {
            self.check_sendable.push((given, location.clone()));
        }

        let mut scope = Environment::new(
            given.type_arguments(&state.db),
            self.type_arguments.clone(),
        );

        if !TypeChecker::new(&state.db).run(given, expected, &mut scope) {
            state.diagnostics.type_error(
                format_type_with_arguments(&state.db, &scope.left, given),
                format_type_with_arguments(&state.db, &scope.right, expected),
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn check_sendable(&mut self, state: &mut State, location: &SourceLocation) {
        if !self.require_sendable {
            return;
        }

        // It's safe to pass `ref T` as an argument if all arguments and `self`
        // are immutable, as this prevents storing of the `ref T` in `self`,
        // thus violating the uniqueness constraints.
        let ref_safe = self.method.is_immutable(&state.db)
            && self.check_sendable.iter().all(|(typ, _)| {
                typ.is_sendable(&state.db) || typ.is_ref(&state.db)
            });

        for (given, loc) in &self.check_sendable {
            if given.is_sendable(&state.db)
                || (given.is_ref(&state.db) && ref_safe)
            {
                continue;
            }

            let targs = &self.type_arguments;

            state.diagnostics.unsendable_argument(
                format_type_with_arguments(&state.db, targs, *given),
                self.module.file(&state.db),
                loc.clone(),
            );
        }

        // If `self` and all arguments are immutable, we allow owned return
        // values provided they don't contain any references. This is safe
        // because `self` can't have references to it (because it's immutable),
        // we can't "leak" a reference through the arguments (because they too
        // are immutable), and the returned value can't refer to `self` because
        // we don't allow references anywhere in the type or its sub types.
        let ret_sendable = if ref_safe {
            self.return_type.is_sendable_output(&state.db)
        } else {
            self.return_type.is_sendable(&state.db)
        };

        if !ret_sendable {
            state.diagnostics.unsendable_return_type(
                format_type_with_arguments(
                    &state.db,
                    &self.type_arguments,
                    self.return_type,
                ),
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn resolve_return_type(&mut self, state: &mut State) -> TypeRef {
        let raw = self.method.return_type(&state.db);
        let typ = TypeResolver::new(
            &mut state.db,
            &self.type_arguments,
            &self.bounds,
        )
        .resolve(raw);

        self.return_type = typ;
        typ
    }
}

/// A compiler pass for type-checking constant definitions.
pub(crate) struct DefineConstants<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineConstants<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut [hir::Module],
    ) -> bool {
        // Regular constants must be defined first such that complex constants
        // (e.g. `A + B` or `[A, B]`) can refer to them, regardless of the order
        // in which modules are processed.
        for module in modules.iter_mut() {
            DefineConstants { state, module: module.module_id }
                .run(module, true);
        }

        for module in modules.iter_mut() {
            DefineConstants { state, module: module.module_id }
                .run(module, false);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module, simple_only: bool) {
        for expression in module.expressions.iter_mut() {
            let node = if let hir::TopLevelExpression::Constant(ref mut node) =
                expression
            {
                node
            } else {
                continue;
            };

            if node.value.is_simple_literal() == simple_only {
                self.define_constant(node);
            }
        }
    }

    fn define_constant(&mut self, node: &mut hir::DefineConstant) {
        let id = node.constant_id.unwrap();
        let typ = CheckConstant::new(self.state, self.module)
            .expression(&mut node.value);

        id.set_value_type(self.db_mut(), typ);
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

/// A compiler pass for type-checking expressions in methods.
pub(crate) struct Expressions<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> Expressions<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            Expressions { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expression in module.expressions.iter_mut() {
            match expression {
                hir::TopLevelExpression::Class(ref mut n) => {
                    self.define_class(n);
                }
                hir::TopLevelExpression::Trait(ref mut n) => {
                    self.define_trait(n);
                }
                hir::TopLevelExpression::Reopen(ref mut n) => {
                    self.reopen_class(n);
                }
                hir::TopLevelExpression::Implement(ref mut n) => {
                    self.implement_trait(n);
                }
                hir::TopLevelExpression::ModuleMethod(ref mut n) => {
                    self.define_module_method(n);
                }
                _ => {}
            }
        }
    }

    fn define_class(&mut self, node: &mut hir::DefineClass) {
        let id = node.class_id.unwrap();
        let num_methods = id.number_of_methods(self.db());

        if num_methods > METHODS_IN_CLASS_LIMIT {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The number of methods defined in this class ({}) \
                    exceeds the maximum of {} methods",
                    num_methods, METHODS_IN_CLASS_LIMIT
                ),
                self.module.file(self.db()),
                node.location.clone(),
            );
        }

        self.verify_type_parameter_requirements(&node.type_parameters);

        for node in &mut node.body {
            match node {
                hir::ClassExpression::AsyncMethod(ref mut n) => {
                    self.define_async_method(n);
                }
                hir::ClassExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(n);
                }
                hir::ClassExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(n);
                }
                _ => {}
            }
        }
    }

    fn reopen_class(&mut self, node: &mut hir::ReopenClass) {
        for node in &mut node.body {
            match node {
                hir::ReopenClassExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(n)
                }
                hir::ReopenClassExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(n)
                }
                hir::ReopenClassExpression::AsyncMethod(ref mut n) => {
                    self.define_async_method(n)
                }
            }
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        self.verify_type_parameter_requirements(&node.type_parameters);
        self.verify_required_traits(
            &node.requirements,
            node.trait_id.unwrap().required_traits(self.db()),
        );

        for node in &mut node.body {
            if let hir::TraitExpression::InstanceMethod(ref mut n) = node {
                self.define_instance_method(n);
            }
        }
    }

    fn implement_trait(&mut self, node: &mut hir::ImplementTrait) {
        for n in &mut node.body {
            self.define_instance_method(n);
        }
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let method = node.method_id.unwrap();
        let stype = method.receiver_id(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns);

        self.verify_type_parameter_requirements(&node.type_parameters);

        for arg in method.arguments(self.db()) {
            scope.variables.add_variable(arg.name, arg.variable);
        }

        let mut checker = CheckMethodBody::new(
            self.state,
            self.module,
            method,
            stype,
            &bounds,
        );

        checker.expressions_with_return(
            returns,
            &mut node.body,
            &mut scope,
            &node.location,
        );
    }

    fn define_instance_method(&mut self, node: &mut hir::DefineInstanceMethod) {
        let method = node.method_id.unwrap();
        let bounds = method.bounds(self.db()).clone();
        let stype = method.receiver_id(self.db());
        let receiver = method.receiver(self.db());
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns);

        self.verify_type_parameter_requirements(&node.type_parameters);

        for arg in method.arguments(self.db()) {
            scope.variables.add_variable(arg.name, arg.variable);
        }

        self.define_field_types(receiver, method, &bounds);

        let mut checker = CheckMethodBody::new(
            self.state,
            self.module,
            method,
            stype,
            &bounds,
        );

        checker.expressions_with_return(
            returns,
            &mut node.body,
            &mut scope,
            &node.location,
        );
    }

    fn define_async_method(&mut self, node: &mut hir::DefineAsyncMethod) {
        let method = node.method_id.unwrap();
        let stype = method.receiver_id(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns = TypeRef::nil();
        let mut scope = LexicalScope::method(receiver, returns);

        self.verify_type_parameter_requirements(&node.type_parameters);

        for arg in method.arguments(self.db()) {
            scope.variables.add_variable(arg.name, arg.variable);
        }

        self.define_field_types(receiver, method, &bounds);

        let mut checker = CheckMethodBody::new(
            self.state,
            self.module,
            method,
            stype,
            &bounds,
        );

        checker.expressions_with_return(
            returns,
            &mut node.body,
            &mut scope,
            &node.location,
        );
    }

    fn define_static_method(&mut self, node: &mut hir::DefineStaticMethod) {
        let method = node.method_id.unwrap();
        let stype = method.receiver_id(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns);

        self.verify_type_parameter_requirements(&node.type_parameters);

        for arg in method.arguments(self.db()) {
            scope.variables.add_variable(arg.name, arg.variable);
        }

        let mut checker = CheckMethodBody::new(
            self.state,
            self.module,
            method,
            stype,
            &bounds,
        );

        checker.expressions_with_return(
            returns,
            &mut node.body,
            &mut scope,
            &node.location,
        );
    }

    fn define_field_types(
        &mut self,
        receiver: TypeRef,
        method: MethodId,
        bounds: &TypeBounds,
    ) {
        for field in receiver.fields(self.db()) {
            let name = field.name(self.db()).clone();
            let raw_type = field.value_type(self.db());
            let args = TypeArguments::new();
            let typ = TypeResolver::new(self.db_mut(), &args, bounds)
                .with_rigid(true)
                .resolve(raw_type);

            method.set_field_type(self.db_mut(), name, field, typ);
        }
    }

    fn verify_type_parameter_requirements(
        &mut self,
        nodes: &[hir::TypeParameter],
    ) {
        for param in nodes {
            self.verify_required_traits(
                &param.requirements,
                param.type_parameter_id.unwrap().requirements(self.db()),
            );
        }
    }

    fn verify_required_traits(
        &mut self,
        nodes: &Vec<hir::TypeName>,
        required_traits: Vec<TraitInstance>,
    ) {
        let mut all_methods = HashMap::new();
        let reqs: HashMap<String, TraitId> = required_traits
            .into_iter()
            .map(|ins| {
                (ins.instance_of().name(self.db()).clone(), ins.instance_of())
            })
            .collect();

        for req in nodes {
            let mut conflicts_with = None;
            let req_id = *reqs.get(&req.name.name).unwrap();
            let methods = req_id
                .required_methods(self.db())
                .into_iter()
                .chain(req_id.default_methods(self.db()))
                .collect::<Vec<_>>();

            for method in methods {
                let name = method.name(self.db());

                if let Some(id) = all_methods.get(name).cloned() {
                    conflicts_with = Some(id);

                    break;
                } else {
                    all_methods.insert(name.clone(), req_id);
                }
            }

            if let Some(id) = conflicts_with {
                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!(
                        "The traits '{}' and '{}' both define a \
                        method with the same name",
                        format_type(self.db(), id),
                        format_type(self.db(), req_id),
                    ),
                    self.module.file(self.db()),
                    req.location.clone(),
                );
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

/// A visitor for type-checking a constant expression.
struct CheckConstant<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> CheckConstant<'a> {
    fn new(state: &'a mut State, module: ModuleId) -> Self {
        Self { state, module }
    }

    fn expression(&mut self, node: &mut hir::ConstExpression) -> TypeRef {
        match node {
            hir::ConstExpression::Int(ref mut n) => self.int_literal(n),
            hir::ConstExpression::Float(ref mut n) => self.float_literal(n),
            hir::ConstExpression::String(ref mut n) => self.string_literal(n),
            hir::ConstExpression::Binary(ref mut n) => self.binary(n),
            hir::ConstExpression::ConstantRef(ref mut n) => self.constant(n),
            hir::ConstExpression::Array(ref mut n) => self.array(n),
            _ => TypeRef::Error,
        }
    }

    fn int_literal(&mut self, node: &mut hir::IntLiteral) -> TypeRef {
        node.resolved_type = TypeRef::int();
        node.resolved_type
    }

    fn float_literal(&mut self, node: &mut hir::FloatLiteral) -> TypeRef {
        node.resolved_type = TypeRef::float();
        node.resolved_type
    }

    fn string_literal(
        &mut self,
        node: &mut hir::ConstStringLiteral,
    ) -> TypeRef {
        if node.value.len() > STRING_LITERAL_LIMIT {
            self.state.diagnostics.string_literal_too_large(
                STRING_LITERAL_LIMIT,
                self.file(),
                node.location.clone(),
            );
        }

        node.resolved_type = TypeRef::string();
        node.resolved_type
    }

    fn binary(&mut self, node: &mut hir::ConstBinary) -> TypeRef {
        let left = self.expression(&mut node.left);
        let name = node.operator.method_name();
        let (left_id, method) = if let Some(found) =
            self.lookup_method(left, name, &node.location)
        {
            found
        } else {
            return TypeRef::Error;
        };

        let mut call = MethodCall::new(
            self.state,
            self.module,
            None,
            left,
            left_id,
            method,
        );

        call.check_mutability(self.state, &node.location);
        call.check_type_bounds(self.state, &node.location);
        self.positional_argument(&mut call, &mut node.right);
        call.check_arguments(self.state, &node.location);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);

        node.resolved_type = call.return_type;
        node.resolved_type
    }

    fn constant(&mut self, node: &mut hir::ConstantRef) -> TypeRef {
        let name = &node.name;
        let symbol = if let Some(src) = node.source.as_ref() {
            if let Some(Symbol::Module(module)) =
                self.module.symbol(self.db(), &src.name)
            {
                module.symbol(self.db(), name)
            } else {
                self.state.diagnostics.symbol_not_a_module(
                    &src.name,
                    self.file(),
                    src.location.clone(),
                );

                return TypeRef::Error;
            }
        } else {
            self.module.symbol(self.db(), name)
        };

        match symbol {
            Some(Symbol::Constant(id)) => {
                node.kind = ConstantKind::Constant(id);
                node.resolved_type = id.value_type(self.db());
                node.resolved_type
            }
            Some(_) => {
                self.state.diagnostics.symbol_not_a_value(
                    name,
                    self.file(),
                    node.location.clone(),
                );

                TypeRef::Error
            }
            _ => {
                if let Some(cons) = self.db().builtin_constant(name) {
                    node.kind = ConstantKind::Builtin(cons);
                    cons.return_type()
                } else {
                    self.state.diagnostics.undefined_symbol(
                        name,
                        self.file(),
                        node.location.clone(),
                    );

                    TypeRef::Error
                }
            }
        }
    }

    fn array(&mut self, node: &mut hir::ConstArray) -> TypeRef {
        let types = node
            .values
            .iter_mut()
            .map(|n| self.expression(n))
            .collect::<Vec<_>>();

        if types.len() > 1 {
            let &first = types.first().unwrap();

            for (&typ, node) in types[1..].iter().zip(node.values[1..].iter()) {
                if !TypeChecker::check(self.db(), typ, first) {
                    self.state.diagnostics.type_error(
                        format_type(self.db(), typ),
                        format_type(self.db(), first),
                        self.file(),
                        node.location().clone(),
                    );
                }
            }
        }

        if types.len() > CONST_ARRAY_LIMIT {
            self.state.diagnostics.error(
                DiagnosticId::InvalidConstExpr,
                format!(
                    "Constant arrays are limited to at most {} values",
                    CONST_ARRAY_LIMIT
                ),
                self.file(),
                node.location.clone(),
            );
        }

        // Mutating constant arrays isn't safe, so they're typed as `ref
        // Array[T]` instead of `Array[T]`.
        let ary = TypeRef::Ref(TypeId::ClassInstance(
            ClassInstance::with_types(self.db_mut(), ClassId::array(), types),
        ));

        node.resolved_type = ary;
        node.resolved_type
    }

    fn lookup_method(
        &mut self,
        receiver: TypeRef,
        name: &str,
        location: &SourceLocation,
    ) -> Option<(TypeId, MethodId)> {
        let rec_id = match receiver.type_id(self.db()) {
            Ok(id) => id,
            Err(TypeRef::Error) => return None,
            Err(typ) => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type(self.db(), typ),
                    self.file(),
                    location.clone(),
                );

                return None;
            }
        };

        match rec_id.lookup_method(self.db(), name, self.module, false) {
            MethodLookup::Ok(id) => return Some((rec_id, id)),
            MethodLookup::Private => {
                self.state.diagnostics.private_method_call(
                    name,
                    self.file(),
                    location.clone(),
                );
            }
            MethodLookup::InstanceOnStatic => {
                self.state.diagnostics.invalid_instance_call(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location.clone(),
                );
            }
            MethodLookup::StaticOnInstance => {
                self.state.diagnostics.invalid_static_call(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location.clone(),
                );
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location.clone(),
                );
            }
        }

        None
    }

    fn positional_argument(
        &mut self,
        call: &mut MethodCall,
        node: &mut hir::ConstExpression,
    ) {
        call.arguments += 1;

        let given = self.expression(node);

        if let Some(expected) =
            call.method.positional_argument_input_type(self.db(), 0)
        {
            call.check_argument(self.state, given, expected, node.location());
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

/// A visitor for type-checking the bodies of methods.
struct CheckMethodBody<'a> {
    state: &'a mut State,

    /// The module the method is defined in.
    module: ModuleId,

    /// The surrounding method.
    method: MethodId,

    /// The type ID of the receiver of the surrounding method.
    self_type: TypeId,

    /// Any bounds to apply to type parameters.
    bounds: &'a TypeBounds,
}

impl<'a> CheckMethodBody<'a> {
    fn new(
        state: &'a mut State,
        module: ModuleId,
        method: MethodId,
        self_type: TypeId,
        bounds: &'a TypeBounds,
    ) -> Self {
        Self { state, module, method, self_type, bounds }
    }

    fn expressions(
        &mut self,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
    ) -> Vec<TypeRef> {
        nodes.iter_mut().map(|n| self.expression(n, scope)).collect()
    }

    fn input_expressions(
        &mut self,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
    ) -> Vec<TypeRef> {
        nodes.iter_mut().map(|n| self.input_expression(n, scope)).collect()
    }

    fn last_expression_type(
        &mut self,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
    ) -> TypeRef {
        self.expressions(nodes, scope)
            .pop()
            .unwrap_or_else(TypeRef::nil)
            .value_type_as_owned(self.db())
    }

    fn expressions_with_return(
        &mut self,
        returns: TypeRef,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
        fallback_location: &SourceLocation,
    ) {
        let typ = self.last_expression_type(nodes, scope);

        if returns.is_nil(self.db()) {
            // When the return type is `Nil` (explicit or not), we just ignore
            // whatever value is returned.
            return;
        }

        if !TypeChecker::check(self.db(), typ, returns) {
            let loc =
                nodes.last().map(|n| n.location()).unwrap_or(fallback_location);

            self.state.diagnostics.type_error(
                format_type(self.db(), typ),
                format_type(self.db(), returns),
                self.file(),
                loc.clone(),
            );
        }
    }

    fn expression(
        &mut self,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        match node {
            hir::Expression::And(ref mut n) => self.and_expression(n, scope),
            hir::Expression::Array(ref mut n) => self.array_literal(n, scope),
            hir::Expression::AssignField(ref mut n) => {
                self.assign_field(n, scope)
            }
            hir::Expression::ReplaceField(ref mut n) => {
                self.replace_field(n, scope)
            }
            hir::Expression::AssignSetter(ref mut n) => {
                self.assign_setter(n, scope)
            }
            hir::Expression::AssignVariable(ref mut n) => {
                self.assign_variable(n, scope)
            }
            hir::Expression::ReplaceVariable(ref mut n) => {
                self.replace_variable(n, scope)
            }
            hir::Expression::Break(ref n) => self.break_expression(n, scope),
            hir::Expression::BuiltinCall(ref mut n) => {
                self.builtin_call(n, scope)
            }
            hir::Expression::Call(ref mut n) => self.call(n, scope),
            hir::Expression::Closure(ref mut n) => self.closure(n, None, scope),
            hir::Expression::ConstantRef(ref mut n) => {
                self.constant(n, scope, false)
            }
            hir::Expression::DefineVariable(ref mut n) => {
                self.define_variable(n, scope)
            }
            hir::Expression::False(ref mut n) => self.false_literal(n),
            hir::Expression::FieldRef(ref mut n) => self.field(n, scope),
            hir::Expression::Float(ref mut n) => self.float_literal(n, scope),
            hir::Expression::IdentifierRef(ref mut n) => {
                self.identifier(n, scope, false)
            }
            hir::Expression::ClassLiteral(ref mut n) => {
                self.class_literal(n, scope)
            }
            hir::Expression::Int(ref mut n) => self.int_literal(n, scope),
            hir::Expression::Loop(ref mut n) => self.loop_expression(n, scope),
            hir::Expression::Match(ref mut n) => {
                self.match_expression(n, scope)
            }
            hir::Expression::Next(ref n) => self.next_expression(n, scope),
            hir::Expression::Or(ref mut n) => self.or_expression(n, scope),
            hir::Expression::Ref(ref mut n) => self.ref_expression(n, scope),
            hir::Expression::Mut(ref mut n) => self.mut_expression(n, scope),
            hir::Expression::Recover(ref mut n) => {
                self.recover_expression(n, scope)
            }
            hir::Expression::Return(ref mut n) => {
                self.return_expression(n, scope)
            }
            hir::Expression::Scope(ref mut n) => self.scope(n, scope),
            hir::Expression::SelfObject(ref mut n) => {
                self.self_expression(n, scope)
            }
            hir::Expression::String(ref mut n) => self.string_literal(n, scope),
            hir::Expression::Throw(ref mut n) => {
                self.throw_expression(n, scope)
            }
            hir::Expression::True(ref mut n) => self.true_literal(n),
            hir::Expression::Nil(ref mut n) => self.nil_literal(n),
            hir::Expression::Tuple(ref mut n) => self.tuple_literal(n, scope),
            hir::Expression::TypeCast(ref mut n) => self.type_cast(n, scope),
            hir::Expression::Try(ref mut n) => self.try_expression(n, scope),
        }
    }

    fn input_expression(
        &mut self,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let typ = self.expression(node, scope);

        if typ.is_uni(self.db()) {
            // This ensures that value types such as `uni T` aren't implicitly
            // converted to `T`.
            return typ;
        }

        if typ.is_value_type(self.db()) {
            return typ.as_owned(self.db());
        }

        typ
    }

    fn argument_expression(
        &mut self,
        expected_type: TypeRef,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
        type_arguments: &TypeArguments,
    ) -> TypeRef {
        match node {
            hir::Expression::Closure(ref mut n) => {
                let expected = expected_type
                    .closure_id(self.db())
                    .map(|f| (f, expected_type, type_arguments));

                self.closure(n, expected, scope)
            }
            _ => self.expression(node, scope),
        }
    }

    fn true_literal(&mut self, node: &mut hir::True) -> TypeRef {
        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn false_literal(&mut self, node: &mut hir::False) -> TypeRef {
        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn nil_literal(&mut self, node: &mut hir::Nil) -> TypeRef {
        node.resolved_type = TypeRef::nil();
        node.resolved_type
    }

    fn int_literal(
        &mut self,
        node: &mut hir::IntLiteral,
        _: &mut LexicalScope,
    ) -> TypeRef {
        node.resolved_type = TypeRef::int();
        node.resolved_type
    }

    fn float_literal(
        &mut self,
        node: &mut hir::FloatLiteral,
        _: &mut LexicalScope,
    ) -> TypeRef {
        node.resolved_type = TypeRef::float();
        node.resolved_type
    }

    fn string_literal(
        &mut self,
        node: &mut hir::StringLiteral,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        for value in &mut node.values {
            match value {
                hir::StringValue::Expression(v) => {
                    let val = self.call(v, scope);

                    if val != TypeRef::Error && !val.is_string(self.db()) {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidType,
                            format!(
                                "Expected a 'String', 'ref String' or \
                                'mut String', found '{}' instead",
                                format_type(self.db(), val)
                            ),
                            self.file(),
                            v.location.clone(),
                        );
                    }
                }
                hir::StringValue::Text(node) => {
                    if node.value.len() > STRING_LITERAL_LIMIT {
                        self.state.diagnostics.string_literal_too_large(
                            STRING_LITERAL_LIMIT,
                            self.file(),
                            node.location.clone(),
                        );
                    }
                }
            }
        }

        node.resolved_type = TypeRef::string();
        node.resolved_type
    }

    fn array_literal(
        &mut self,
        node: &mut hir::ArrayLiteral,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let types = self.input_expressions(&mut node.values, scope);

        if types.len() > 1 {
            let &first = types.first().unwrap();

            for (&typ, node) in types[1..].iter().zip(node.values[1..].iter()) {
                if !TypeChecker::check(self.db(), typ, first) {
                    self.state.diagnostics.type_error(
                        format_type(self.db(), typ),
                        format_type(self.db(), first),
                        self.file(),
                        node.location().clone(),
                    );
                }
            }
        }

        // Since other types aren't compatible with Any, we only need to check
        // the first value's type.
        if !types.is_empty() && types[0].is_any(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                "Arrays can't store values of type 'Any'",
                self.file(),
                node.location.clone(),
            );
        }

        let ins =
            ClassInstance::with_types(self.db_mut(), ClassId::array(), types);
        let ary = TypeRef::Owned(TypeId::ClassInstance(ins));

        node.value_type =
            ins.first_type_argument(self.db()).unwrap_or(TypeRef::Unknown);
        node.resolved_type = ary;
        node.resolved_type
    }

    fn tuple_literal(
        &mut self,
        node: &mut hir::TupleLiteral,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let types = self.input_expressions(&mut node.values, scope);
        let class = if let Some(id) = ClassId::tuple(types.len()) {
            id
        } else {
            self.state
                .diagnostics
                .tuple_size_error(self.file(), node.location.clone());

            return TypeRef::Error;
        };

        let tuple = TypeRef::Owned(TypeId::ClassInstance(
            ClassInstance::with_types(self.db_mut(), class, types.clone()),
        ));

        node.class_id = Some(class);
        node.resolved_type = tuple;
        node.value_types = types;
        node.resolved_type
    }

    fn class_literal(
        &mut self,
        node: &mut hir::ClassLiteral,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let name = &node.class_name.name;
        let class = if name == "Self" {
            match scope.surrounding_type {
                TypeRef::Owned(TypeId::Class(id)) => id,
                TypeRef::Owned(TypeId::ClassInstance(ins))
                | TypeRef::Ref(TypeId::ClassInstance(ins)) => ins.instance_of(),
                _ => {
                    self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    "'Self' is only available to methods defined for a class",
                    self.file(),
                    node.class_name.location.clone(),
                );

                    return TypeRef::Error;
                }
            }
        } else if let Some(Symbol::Class(id)) =
            self.module.symbol(self.db(), name)
        {
            id
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!("'{}' isn't a class", name),
                self.file(),
                node.class_name.location.clone(),
            );

            return TypeRef::Error;
        };

        let require_send = class.kind(self.db()).is_async();
        let ins = ClassInstance::empty(self.db_mut(), class);
        let mut assigned = HashSet::new();

        for field in &mut node.fields {
            let name = &field.field.name;
            let field_id = if let Some(id) = class.field(self.db(), name) {
                id
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!("The field '{}' is undefined", name),
                    self.file(),
                    field.field.location.clone(),
                );

                continue;
            };

            if !field_id.is_public(self.db())
                && class.module(self.db()) != self.module
            {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!("The field '{}' is private", name),
                    self.file(),
                    node.location.clone(),
                );
            }

            let expected = field_id.value_type(self.db());
            let value = self.expression(&mut field.value, scope);
            let value_casted = value.cast_according_to(expected, self.db());
            let checker = TypeChecker::new(self.db());
            let mut env = Environment::new(
                value_casted.type_arguments(self.db()),
                ins.type_arguments(self.db()).clone(),
            );

            if !checker.run(value_casted, expected, &mut env) {
                self.state.diagnostics.type_error(
                    format_type_with_arguments(self.db(), &env.left, value),
                    format_type_with_arguments(self.db(), &env.right, expected),
                    self.file(),
                    field.value.location().clone(),
                );
            }

            if require_send && !value.is_sendable(self.db()) {
                self.state.diagnostics.unsendable_field_value(
                    name,
                    format_type(self.db(), value),
                    self.file(),
                    field.value.location().clone(),
                );
            }

            if assigned.contains(name) {
                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!("The field '{}' is already assigned", name),
                    self.file(),
                    field.field.location.clone(),
                );
            }

            field.field_id = Some(field_id);
            field.resolved_type = expected;

            assigned.insert(name);
        }

        for field in class.field_names(self.db()) {
            if assigned.contains(&field) {
                continue;
            }

            self.state.diagnostics.error(
                DiagnosticId::MissingField,
                format!("The field '{}' must be assigned a value", field),
                self.file(),
                node.location.clone(),
            );
        }

        node.class_id = Some(class);
        node.resolved_type = TypeRef::Owned(TypeId::ClassInstance(ins));
        node.resolved_type
    }

    fn self_expression(
        &mut self,
        node: &mut hir::SelfObject,
        scope: &LexicalScope,
    ) -> TypeRef {
        let typ = scope.surrounding_type;

        if !self.method.is_instance_method(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidSymbol,
                "'self' can only be used in instance methods",
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        if scope.in_recover() && !typ.is_sendable(self.db()) {
            self.state.diagnostics.unsendable_type_in_recover(
                self.fmt(typ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        scope.mark_closures_as_capturing_self(self.db_mut());

        node.resolved_type = typ;
        node.resolved_type
    }

    fn scope(
        &mut self,
        node: &mut hir::Scope,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let mut new_scope = scope.inherit(ScopeKind::Regular);
        let last_type =
            self.last_expression_type(&mut node.body, &mut new_scope);

        node.resolved_type = last_type;
        node.resolved_type
    }

    fn define_variable(
        &mut self,
        node: &mut hir::DefineVariable,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let value_type = self.input_expression(&mut node.value, scope);

        if !value_type.allow_assignment(self.db()) {
            self.state.diagnostics.cant_assign_type(
                &format_type(self.db(), value_type),
                self.file(),
                node.value.location().clone(),
            );
        }

        let var_type = if let Some(tnode) = node.value_type.as_mut() {
            let exp_type = self.type_signature(tnode, self.self_type);
            let value_casted =
                value_type.cast_according_to(exp_type, self.db());

            if !TypeChecker::check(self.db(), value_casted, exp_type) {
                self.state.diagnostics.type_error(
                    format_type(self.db(), value_type),
                    format_type(self.db(), exp_type),
                    self.file(),
                    node.value.location().clone(),
                );
            }

            exp_type
        } else {
            value_type
        };

        let name = &node.name.name;
        let rtype = TypeRef::nil();

        node.resolved_type = var_type;

        if name == IGNORE_VARIABLE {
            return rtype;
        }

        let id = scope.variables.new_variable(
            self.db_mut(),
            name.clone(),
            var_type,
            node.mutable,
        );

        node.variable_id = Some(id);
        rtype
    }

    fn pattern(
        &mut self,
        node: &mut hir::Pattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        match node {
            hir::Pattern::Identifier(ref mut n) => {
                self.identifier_pattern(n, value_type, pattern);
            }
            hir::Pattern::Tuple(ref mut n) => {
                self.tuple_pattern(n, value_type, pattern);
            }
            hir::Pattern::Class(ref mut n) => {
                self.class_pattern(n, value_type, pattern);
            }
            hir::Pattern::Int(ref mut n) => {
                self.int_pattern(n, value_type);
            }
            hir::Pattern::String(ref mut n) => {
                self.string_pattern(n, value_type);
            }
            hir::Pattern::True(ref mut n) => {
                self.true_pattern(n, value_type);
            }
            hir::Pattern::False(ref mut n) => {
                self.false_pattern(n, value_type);
            }
            hir::Pattern::Constant(ref mut n) => {
                self.constant_pattern(n, value_type);
            }
            hir::Pattern::Variant(ref mut n) => {
                self.variant_pattern(n, value_type, pattern);
            }
            hir::Pattern::Wildcard(_) => {
                // Nothing to do for wildcards, as we just ignore the value.
            }
            hir::Pattern::Or(ref mut n) => {
                self.or_pattern(n, value_type, pattern);
            }
        }
    }

    fn identifier_pattern(
        &mut self,
        node: &mut hir::IdentifierPattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        let var_type = if let Some(tnode) = node.value_type.as_mut() {
            let exp_type = self.type_signature(tnode, self.self_type);

            if !TypeChecker::check(self.db(), value_type, exp_type) {
                self.state.diagnostics.pattern_type_error(
                    format_type(self.db(), value_type),
                    format_type(self.db(), exp_type),
                    self.file(),
                    node.location.clone(),
                );
            }

            exp_type
        } else {
            value_type
        };

        let name = node.name.name.clone();

        if name == IGNORE_VARIABLE {
            return;
        }

        if pattern.variables.contains_key(&name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                node.location.clone(),
            );
        }

        if let Some(existing) = pattern.variable_scope.variable(&name) {
            let ex_type = existing.value_type(self.db());

            if !TypeChecker::check(self.db(), var_type, ex_type) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "The type of this variable is defined as '{}' \
                        in another pattern, but here its type is '{}'",
                        format_type(self.db(), ex_type),
                        format_type(self.db(), var_type),
                    ),
                    self.file(),
                    node.location.clone(),
                );
            }

            if existing.is_mutable(self.db()) != node.mutable {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidPattern,
                    "The mutability of this binding must be the same \
                    in all patterns",
                    self.file(),
                    node.location.clone(),
                );
            }

            node.variable_id = Some(existing);

            pattern.variables.insert(name, existing);
            return;
        }

        let id = pattern.variable_scope.new_variable(
            self.db_mut(),
            name.clone(),
            var_type,
            node.mutable,
        );

        node.variable_id = Some(id);

        pattern.variables.insert(name, id);
    }

    fn constant_pattern(
        &mut self,
        node: &mut hir::ConstantPattern,
        value_type: TypeRef,
    ) {
        let name = &node.name;

        if let Some(ins) = value_type.as_enum_instance(self.db()) {
            let variant =
                if let Some(v) = ins.instance_of().variant(self.db(), name) {
                    v
                } else {
                    self.state.diagnostics.undefined_variant(
                        name,
                        format_type(self.db(), value_type),
                        self.file(),
                        node.location.clone(),
                    );

                    return;
                };

            let members = variant.members(self.db());

            if !members.is_empty() {
                self.state.diagnostics.incorrect_pattern_arguments(
                    0,
                    members.len(),
                    self.file(),
                    node.location.clone(),
                );

                return;
            }

            node.kind = ConstantPatternKind::Variant(variant);

            return;
        }

        let symbol = self.lookup_constant(name, node.source.as_ref());
        let exp_type = match symbol {
            Ok(Some(Symbol::Constant(id))) => {
                let typ = id.value_type(self.db());

                node.kind = if typ.is_int(self.db()) {
                    ConstantPatternKind::Int(id)
                } else if typ.is_string(self.db()) {
                    ConstantPatternKind::String(id)
                } else {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidPattern,
                        format!(
                            "Expected a 'String' or 'Int', found '{}' instead",
                            format_type(self.db(), typ),
                        ),
                        self.file(),
                        node.location.clone(),
                    );

                    return;
                };

                typ
            }
            Ok(Some(_)) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!("The symbol '{}' is not a constant", name),
                    self.file(),
                    node.location.clone(),
                );

                return;
            }
            Ok(None) => {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    node.location.clone(),
                );

                return;
            }
            Err(_) => {
                return;
            }
        };

        if !TypeChecker::check(self.db(), value_type, exp_type) {
            self.state.diagnostics.pattern_type_error(
                format_type(self.db(), value_type),
                format_type(self.db(), exp_type),
                self.file(),
                node.location.clone(),
            );
        }
    }

    fn tuple_pattern(
        &mut self,
        node: &mut hir::TuplePattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        if value_type == TypeRef::Error {
            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let ins = match value_type {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
                if ins.instance_of().kind(self.db()).is_tuple() =>
            {
                ins
            }
            _ => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "This pattern expects a tuple, \
                        but the input type is '{}'",
                        format_type(self.db(), value_type),
                    ),
                    self.file(),
                    node.location.clone(),
                );

                self.error_patterns(&mut node.values, pattern);
                return;
            }
        };

        let params = ins.instance_of().number_of_type_parameters(self.db());

        if params != node.values.len() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "This pattern requires {} tuple members, \
                    but the input has {} members",
                    params,
                    node.values.len()
                ),
                self.file(),
                node.location.clone(),
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let raw_types = ins.ordered_type_arguments(self.db());
        let mut values = Vec::with_capacity(raw_types.len());
        let fields = ins.instance_of().fields(self.db());

        for (patt, vtype) in node.values.iter_mut().zip(raw_types.into_iter()) {
            let typ = vtype.cast_according_to(value_type, self.db());

            self.pattern(patt, typ, pattern);
            values.push(typ);
        }

        node.field_ids = fields;
    }

    fn class_pattern(
        &mut self,
        node: &mut hir::ClassPattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        if value_type == TypeRef::Error {
            self.field_error_patterns(&mut node.values, pattern);
            return;
        }

        let ins = match value_type {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
                if ins
                    .instance_of()
                    .kind(self.db())
                    .allow_pattern_matching() =>
            {
                ins
            }
            _ => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "A regular or extern class instance is expected, \
                        but the input type is an instance of type '{}'",
                        format_type(self.db(), value_type),
                    ),
                    self.file(),
                    node.location.clone(),
                );

                self.field_error_patterns(&mut node.values, pattern);
                return;
            }
        };

        let class = ins.instance_of();

        if class.has_destructor(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type '{}' can't be destructured as it defines \
                    a custom destructor",
                    format_type(self.db(), value_type)
                ),
                self.file(),
                node.location.clone(),
            );
        }

        if class.kind(self.db()).is_enum() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                "Enum classes don't support class patterns",
                self.file(),
                node.location.clone(),
            );
        }

        let immutable = value_type.is_ref(self.db());
        let args = TypeArguments::for_class(self.db(), ins);

        for node in &mut node.values {
            let name = &node.field.name;
            let field = if let Some(f) = class.field(self.db(), name) {
                f
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "The type '{}' doesn't define the field '{}'",
                        format_type(self.db(), value_type),
                        name
                    ),
                    self.file(),
                    node.field.location.clone(),
                );

                self.pattern(&mut node.pattern, TypeRef::Error, pattern);
                continue;
            };

            let raw_type = field.value_type(self.db());
            let field_type =
                TypeResolver::new(&mut self.state.db, &args, self.bounds)
                    .with_immutable(immutable)
                    .resolve(raw_type)
                    .cast_according_to(value_type, self.db());

            node.field_id = Some(field);

            self.pattern(&mut node.pattern, field_type, pattern);
        }

        node.class_id = Some(class);
    }

    fn int_pattern(&mut self, node: &mut hir::IntLiteral, input_type: TypeRef) {
        let typ = TypeRef::int();

        self.expression_pattern(typ, input_type, &node.location);
    }

    fn string_pattern(
        &mut self,
        node: &mut hir::StringPattern,
        input_type: TypeRef,
    ) {
        let typ = TypeRef::string();

        self.expression_pattern(typ, input_type, &node.location);
    }

    fn true_pattern(&mut self, node: &mut hir::True, input_type: TypeRef) {
        let typ = TypeRef::boolean();

        self.expression_pattern(typ, input_type, &node.location);
    }

    fn false_pattern(&mut self, node: &mut hir::False, input_type: TypeRef) {
        let typ = TypeRef::boolean();

        self.expression_pattern(typ, input_type, &node.location);
    }

    fn expression_pattern(
        &mut self,
        pattern_type: TypeRef,
        input_type: TypeRef,
        location: &SourceLocation,
    ) {
        let compare = if input_type.is_owned_or_uni(self.db()) {
            input_type
        } else {
            // This ensures we can compare e.g. a `ref Int` to an integer
            // pattern.
            input_type.as_owned(self.db())
        };

        if !TypeChecker::check(self.db(), compare, pattern_type) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type of this pattern is '{}', \
                    but the input type is '{}'",
                    format_type(self.db(), pattern_type),
                    format_type(self.db(), input_type),
                ),
                self.file(),
                location.clone(),
            );
        }
    }

    fn variant_pattern(
        &mut self,
        node: &mut hir::VariantPattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        if value_type == TypeRef::Error {
            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let ins = if let Some(ins) = value_type.as_enum_instance(self.db()) {
            ins
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "This pattern expects an enum class, \
                    but the input type is '{}'",
                    format_type(self.db(), value_type),
                ),
                self.file(),
                node.location.clone(),
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        };

        let name = &node.name.name;
        let class = ins.instance_of();

        let variant = if let Some(v) = class.variant(self.db(), name) {
            v
        } else {
            self.state.diagnostics.undefined_variant(
                name,
                format_type(self.db(), value_type),
                self.file(),
                node.location.clone(),
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        };

        let members = variant.members(self.db());

        if members.len() != node.values.len() {
            self.state.diagnostics.incorrect_pattern_arguments(
                node.values.len(),
                members.len(),
                self.file(),
                node.location.clone(),
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let immutable = value_type.is_ref(self.db());
        let args = TypeArguments::for_class(self.db(), ins);
        let bounds = self.bounds;

        for (patt, member) in node.values.iter_mut().zip(members.into_iter()) {
            let typ = TypeResolver::new(self.db_mut(), &args, bounds)
                .with_immutable(immutable)
                .resolve(member)
                .cast_according_to(value_type, self.db());

            self.pattern(patt, typ, pattern);
        }

        node.variant_id = Some(variant);
    }

    fn or_pattern(
        &mut self,
        node: &mut hir::OrPattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        let patterns: Vec<_> = node
            .patterns
            .iter_mut()
            .map(|node| {
                let mut new_pattern = Pattern::new(pattern.variable_scope);

                self.pattern(node, value_type, &mut new_pattern);
                (new_pattern.variables, node.location())
            })
            .collect();

        let all_var_names = pattern.variable_scope.names();

        // Now that all patterns have defined their variables, we can check
        // each pattern to ensure they all define the same variables. This
        // is needed as code like `case A(a), B(b) -> test(a)` is invalid,
        // as the variable could be undefined depending on which pattern
        // matched.
        for (vars, location) in &patterns {
            for &name in &all_var_names {
                if vars.contains_key(name) {
                    continue;
                }

                self.state.diagnostics.error(
                    DiagnosticId::InvalidPattern,
                    format!("This pattern must define the variable '{}'", name),
                    self.file(),
                    (*location).clone(),
                );
            }
        }

        // Since we use a sub Pattern for tracking defined variables per OR
        // branch, we have to copy those to the parent Pattern.
        for (key, val) in &pattern.variable_scope.variables {
            pattern.variables.insert(key.clone(), *val);
        }
    }

    fn assign_variable(
        &mut self,
        node: &mut hir::AssignVariable,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((var, _)) = self.check_variable_assignment(
            &node.variable.name,
            &mut node.value,
            &node.variable.location,
            scope,
        ) {
            node.variable_id = Some(var);
            node.resolved_type = TypeRef::nil();
            node.resolved_type
        } else {
            TypeRef::Error
        }
    }

    fn replace_variable(
        &mut self,
        node: &mut hir::ReplaceVariable,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((var, typ)) = self.check_variable_assignment(
            &node.variable.name,
            &mut node.value,
            &node.variable.location,
            scope,
        ) {
            node.variable_id = Some(var);
            node.resolved_type = typ;
            node.resolved_type
        } else {
            TypeRef::Error
        }
    }

    fn check_variable_assignment(
        &mut self,
        name: &str,
        value_node: &mut hir::Expression,
        location: &SourceLocation,
        scope: &mut LexicalScope,
    ) -> Option<(VariableId, TypeRef)> {
        let (var, _, allow_assignment) =
            if let Some(val) = self.lookup_variable(name, scope, location) {
                val
            } else {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    location.clone(),
                );

                return None;
            };

        if !allow_assignment {
            self.state.diagnostics.error(
                DiagnosticId::InvalidAssign,
                "Variables captured by non-moving closures can't be assigned \
                new values"
                    .to_string(),
                self.file(),
                location.clone(),
            );

            return None;
        }

        if !var.is_mutable(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidAssign,
                format!(
                    "The variable '{}' is immutable and can't be \
                    assigned a new value",
                    name
                ),
                self.file(),
                location.clone(),
            );

            return None;
        }

        let val_type = self.expression(value_node, scope);

        if !val_type.allow_assignment(self.db()) {
            self.state.diagnostics.cant_assign_type(
                &format_type(self.db(), val_type),
                self.file(),
                value_node.location().clone(),
            );
        }

        let var_type = var.value_type(self.db());

        if !TypeChecker::check(self.db(), val_type, var_type) {
            self.state.diagnostics.type_error(
                format_type(self.db(), val_type),
                format_type(self.db(), var_type),
                self.file(),
                location.clone(),
            );

            return None;
        }

        Some((var, var_type))
    }

    fn closure(
        &mut self,
        node: &mut hir::Closure,
        mut expected: Option<(ClosureId, TypeRef, &TypeArguments)>,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let self_type = self.self_type;
        let moving = node.moving
            || expected
                .as_ref()
                .map_or(false, |(id, _, _)| id.is_moving(self.db()));

        let closure = Closure::alloc(self.db_mut(), moving);
        let bounds = self.bounds;
        let return_type = if let Some(n) = node.return_type.as_mut() {
            self.type_signature(n, self_type)
        } else {
            let db = self.db_mut();

            expected
                .as_mut()
                .map(|(id, _, targs)| {
                    let raw = id.return_type(db);

                    TypeResolver::new(db, targs, bounds).resolve(raw)
                })
                .unwrap_or_else(|| TypeRef::placeholder(db, None))
        };

        closure.set_return_type(self.db_mut(), return_type);

        let surrounding_type =
            if scope.surrounding_type.is_owned_or_uni(self.db()) {
                scope.surrounding_type.as_mut(self.db())
            } else {
                scope.surrounding_type
            };

        let mut new_scope = LexicalScope {
            kind: ScopeKind::Closure(closure),
            surrounding_type,
            return_type,
            variables: VariableScope::new(),
            parent: Some(scope),
            in_closure: true,
            break_in_loop: Cell::new(false),
        };

        for (index, arg) in node.arguments.iter_mut().enumerate() {
            let name = arg.name.name.clone();
            let typ = if let Some(n) = arg.value_type.as_mut() {
                self.type_signature(n, self.self_type)
            } else {
                let db = self.db_mut();

                expected
                    .as_mut()
                    .and_then(|(id, _, targs)| {
                        id.positional_argument_input_type(db, index).map(|t| {
                            TypeResolver::new(db, targs, bounds).resolve(t)
                        })
                    })
                    .unwrap_or_else(|| TypeRef::placeholder(db, None))
            };

            let var =
                closure.new_argument(self.db_mut(), name.clone(), typ, typ);

            new_scope.variables.add_variable(name, var);
        }

        self.expressions_with_return(
            return_type,
            &mut node.body,
            &mut new_scope,
            &node.location,
        );

        node.resolved_type = match expected.as_ref() {
            // If a closure is immediately passed to a `uni fn`, and we don't
            // capture any variables, we can safely infer the closure as unique.
            // This removes the need for `recover fn { ... }` in most cases
            // where a `uni fn` is needed.
            //
            // `fn move` closures are not inferred as `uni fn`, as the values
            // moved into the closure may still be referred to from elsewhere.
            Some((_, exp, _))
                if exp.is_uni(self.db())
                    && closure.can_infer_as_uni(self.db()) =>
            {
                TypeRef::Uni(TypeId::Closure(closure))
            }
            _ => TypeRef::Owned(TypeId::Closure(closure)),
        };

        node.closure_id = Some(closure);

        node.resolved_type
    }

    fn constant(
        &mut self,
        node: &mut hir::ConstantRef,
        scope: &LexicalScope,
        receiver: bool,
    ) -> TypeRef {
        let name = &node.name;
        let module = self.module;
        let (rec, rec_id, rec_kind, method) = {
            let rec = scope.surrounding_type;
            let rec_id = rec.type_id(self.db()).unwrap();

            match rec_id.lookup_method(self.db(), name, module, false) {
                MethodLookup::Ok(method) => {
                    let rec_info =
                        Receiver::class_or_implicit(self.db(), method);

                    (rec, rec_id, rec_info, method)
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                _ => {
                    let symbol =
                        self.lookup_constant(name, node.source.as_ref());

                    match symbol {
                        Ok(Some(Symbol::Constant(id))) => {
                            node.resolved_type = id.value_type(self.db());
                            node.kind = ConstantKind::Constant(id);

                            return node.resolved_type;
                        }
                        Ok(Some(Symbol::Class(id))) if receiver => {
                            node.resolved_type =
                                TypeRef::Owned(TypeId::Class(id));

                            return node.resolved_type;
                        }
                        Ok(Some(Symbol::Class(_) | Symbol::Trait(_)))
                            if !receiver =>
                        {
                            self.state.diagnostics.symbol_not_a_value(
                                name,
                                self.file(),
                                node.location.clone(),
                            );

                            return TypeRef::Error;
                        }
                        Err(_) => {
                            return TypeRef::Error;
                        }
                        _ => {}
                    }

                    if let Some(Symbol::Method(method)) =
                        module.symbol(self.db(), name)
                    {
                        let id = method.module(self.db());

                        (
                            TypeRef::module(id),
                            TypeId::Module(id),
                            Receiver::Class(id.class(self.db())),
                            method,
                        )
                    } else {
                        self.state.diagnostics.undefined_symbol(
                            name,
                            self.file(),
                            node.location.clone(),
                        );

                        return TypeRef::Error;
                    }
                }
            }
        };

        let mut call = MethodCall::new(
            self.state,
            module,
            Some((self.self_type, self.method)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, &node.location);
        call.check_type_bounds(self.state, &node.location);
        call.check_arguments(self.state, &node.location);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);

        let returns = call.return_type;

        node.kind = ConstantKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        node.resolved_type = returns;
        node.resolved_type
    }

    fn identifier(
        &mut self,
        node: &mut hir::IdentifierRef,
        scope: &mut LexicalScope,
        receiver: bool,
    ) -> TypeRef {
        let name = &node.name;
        let module = self.module;

        if let Some((var, typ, _)) =
            self.lookup_variable(name, scope, &node.location)
        {
            node.kind = IdentifierKind::Variable(var);

            return typ;
        }

        let (rec, rec_id, rec_kind, method) = {
            let rec = scope.surrounding_type;
            let rec_id = rec.type_id(self.db()).unwrap();

            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(method) => {
                    self.check_if_self_is_allowed(scope, &node.location);

                    if method.is_instance_method(self.db()) {
                        scope.mark_closures_as_capturing_self(self.db_mut());
                    }

                    let rec_info =
                        Receiver::class_or_implicit(self.db(), method);

                    (rec, rec_id, rec_info, method)
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::Private => {
                    self.private_method_call(name, &node.location);

                    return TypeRef::Error;
                }
                _ => {
                    if let Some(Symbol::Module(id)) =
                        module.symbol(self.db(), name)
                    {
                        if !receiver {
                            self.state.diagnostics.symbol_not_a_value(
                                name,
                                self.file(),
                                node.location.clone(),
                            );

                            return TypeRef::Error;
                        }

                        return TypeRef::module(id);
                    }

                    if let Some(Symbol::Method(method)) =
                        module.symbol(self.db(), name)
                    {
                        let id = method.module(self.db());

                        (
                            TypeRef::module(id),
                            TypeId::Module(id),
                            Receiver::Class(id.class(self.db())),
                            method,
                        )
                    } else {
                        self.state.diagnostics.undefined_symbol(
                            name,
                            self.file(),
                            node.location.clone(),
                        );

                        return TypeRef::Error;
                    }
                }
            }
        };

        let mut call = MethodCall::new(
            self.state,
            module,
            Some((self.self_type, self.method)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, &node.location);
        call.check_type_bounds(self.state, &node.location);
        call.check_arguments(self.state, &node.location);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);
        let returns = call.return_type;

        node.kind = IdentifierKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn field(
        &mut self,
        node: &mut hir::FieldRef,
        scope: &LexicalScope,
    ) -> TypeRef {
        let name = &node.name;
        let (field, raw_type) = if let Some(typ) = self.field_type(name) {
            typ
        } else {
            self.state.diagnostics.undefined_field(
                name,
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        };

        node.field_id = Some(field);
        node.resolved_type =
            self.field_reference(raw_type, scope, &node.location);

        node.resolved_type
    }

    fn assign_field(
        &mut self,
        node: &mut hir::AssignField,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((field, typ)) = self.check_field_assignment(
            &node.field.name,
            &mut node.value,
            &node.field.location,
            scope,
        ) {
            node.field_id = Some(field);
            node.resolved_type = typ;

            TypeRef::nil()
        } else {
            TypeRef::Error
        }
    }

    fn replace_field(
        &mut self,
        node: &mut hir::ReplaceField,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((field, typ)) = self.check_field_assignment(
            &node.field.name,
            &mut node.value,
            &node.field.location,
            scope,
        ) {
            node.field_id = Some(field);
            node.resolved_type = typ;
            node.resolved_type
        } else {
            TypeRef::Error
        }
    }

    fn check_field_assignment(
        &mut self,
        name: &str,
        value_node: &mut hir::Expression,
        location: &SourceLocation,
        scope: &mut LexicalScope,
    ) -> Option<(FieldId, TypeRef)> {
        let val_type = self.expression(value_node, scope);

        if !val_type.allow_assignment(self.db()) {
            self.state.diagnostics.cant_assign_type(
                &format_type(self.db(), val_type),
                self.file(),
                value_node.location().clone(),
            );
        }

        let (field, var_type) = if let Some(typ) = self.field_type(name) {
            typ
        } else {
            self.state.diagnostics.undefined_field(
                name,
                self.file(),
                location.clone(),
            );

            return None;
        };

        if !TypeChecker::check(self.db(), val_type, var_type) {
            self.state.diagnostics.type_error(
                format_type(self.db(), val_type),
                format_type(self.db(), var_type),
                self.file(),
                location.clone(),
            );
        }

        if !scope.surrounding_type.allow_mutating() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidAssign,
                format!(
                    "Can't assign a new value to field '{}', as the \
                    surrounding method is immutable",
                    name
                ),
                self.file(),
                location.clone(),
            );
        }

        if scope.in_recover() && !var_type.is_sendable(self.db()) {
            self.state.diagnostics.unsendable_type_in_recover(
                self.fmt(var_type),
                self.file(),
                location.clone(),
            );
        }

        scope.mark_closures_as_capturing_self(self.db_mut());
        Some((field, var_type))
    }

    fn loop_expression(
        &mut self,
        node: &mut hir::Loop,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let mut new_scope = scope.inherit(ScopeKind::Loop);

        self.expressions(&mut node.body, &mut new_scope);

        // Loops are expressions like any other. If we don't break out of the
        // loop explicitly we may come to depend on the result of the `loop`
        // expression later (e.g. `if x { loop { break } }`).
        //
        // If we never break out of the loop the return type is `Never` because,
        // well, we'll never reach whatever code comes after the loop.
        if new_scope.break_in_loop.get() {
            TypeRef::nil()
        } else {
            TypeRef::Never
        }
    }

    fn break_expression(
        &mut self,
        node: &hir::Break,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let mut in_loop = false;
        let mut scope = Some(&*scope);

        while let Some(current) = scope {
            if current.kind == ScopeKind::Loop {
                in_loop = true;
                current.break_in_loop.set(true);
                break;
            }

            scope = current.parent;
        }

        if !in_loop {
            self.state.diagnostics.error(
                DiagnosticId::InvalidLoopKeyword,
                "The 'break' keyword can only be used inside loops",
                self.file(),
                node.location.clone(),
            );
        }

        TypeRef::Never
    }

    fn next_expression(
        &mut self,
        node: &hir::Next,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if !scope.in_loop() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidLoopKeyword,
                "The 'next' keyword can only be used inside loops",
                self.file(),
                node.location.clone(),
            );
        }

        TypeRef::Never
    }

    fn and_expression(
        &mut self,
        node: &mut hir::And,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let lhs = self.expression(&mut node.left, scope);
        let rhs = self.expression(&mut node.right, scope);

        self.require_boolean(lhs, node.left.location());
        self.require_boolean(rhs, node.right.location());

        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn or_expression(
        &mut self,
        node: &mut hir::Or,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let lhs = self.expression(&mut node.left, scope);
        let rhs = self.expression(&mut node.right, scope);

        self.require_boolean(lhs, node.left.location());
        self.require_boolean(rhs, node.right.location());

        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn return_expression(
        &mut self,
        node: &mut hir::Return,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let mut returned = node
            .value
            .as_mut()
            .map(|n| self.expression(n, scope))
            .unwrap_or_else(TypeRef::nil);

        if scope.in_recover() && returned.is_owned(self.db()) {
            returned = returned.as_uni(self.db());
        }

        let expected = scope.return_type;

        if !TypeChecker::check(self.db(), returned, expected) {
            self.state.diagnostics.type_error(
                format_type(self.db(), returned),
                format_type(self.db(), expected),
                self.file(),
                node.location.clone(),
            );
        }

        node.resolved_type = returned;
        TypeRef::Never
    }

    fn throw_expression(
        &mut self,
        node: &mut hir::Throw,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr = self.expression(&mut node.value, scope);

        if expr.is_error(self.db()) {
            return expr;
        }

        let ret_type = scope.return_type;

        node.return_type = ret_type;

        match ret_type.throw_kind(self.db()) {
            ThrowKind::Unknown | ThrowKind::Option(_) => self
                .state
                .diagnostics
                .throw_not_available(self.file(), node.location.clone()),
            ThrowKind::Result(ret_ok, ret_err) => {
                node.resolved_type =
                    if scope.in_recover() && expr.is_owned(self.db()) {
                        expr.as_uni(self.db())
                    } else {
                        expr
                    };

                if !TypeChecker::check(self.db(), expr, ret_err) {
                    self.state.diagnostics.invalid_throw(
                        ThrowKind::Result(ret_ok, expr)
                            .throw_type_name(self.db(), ret_ok),
                        format_type(self.db(), ret_type),
                        self.file(),
                        node.location.clone(),
                    );
                }
            }
        }

        TypeRef::Never
    }

    fn match_expression(
        &mut self,
        node: &mut hir::Match,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let input_type = self.expression(&mut node.expression, scope);
        let mut rtype =
            if node.write_result { None } else { Some(TypeRef::nil()) };

        for case in &mut node.cases {
            let mut new_scope = scope.inherit(ScopeKind::Regular);
            let mut pattern = Pattern::new(&mut new_scope.variables);

            self.pattern(&mut case.pattern, input_type, &mut pattern);

            case.variable_ids = pattern.variables.values().cloned().collect();

            if let Some(guard) = case.guard.as_mut() {
                let mut scope = new_scope.inherit(ScopeKind::Regular);
                let typ = self.expression(guard, &mut scope);

                self.require_boolean(typ, guard.location());
            }

            if let Some(expected) = rtype {
                self.expressions_with_return(
                    expected,
                    &mut case.body,
                    &mut new_scope,
                    &case.location,
                );
            } else {
                let typ =
                    self.last_expression_type(&mut case.body, &mut new_scope);

                // If an arm returns a Never type we'll ignore it. This way e.g.
                // the first arm can return `Never` and the other arms can
                // return other types, as long as those types are compatible
                // with the first non-Never type.
                if !typ.is_never(self.db()) {
                    rtype = Some(typ);
                }
            }
        }

        node.resolved_type = rtype.unwrap_or(TypeRef::Error);
        node.resolved_type
    }

    fn ref_expression(
        &mut self,
        node: &mut hir::Ref,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr = self.expression(&mut node.value, scope);

        if !expr.allow_as_ref(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "A 'ref T' can't be created from a value of type '{}'",
                    self.fmt(expr)
                ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        node.resolved_type = if expr.is_value_type(self.db()) {
            expr
        } else {
            expr.as_ref(self.db())
        };

        node.resolved_type
    }

    fn mut_expression(
        &mut self,
        node: &mut hir::Mut,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr = self.expression(&mut node.value, scope);

        if !expr.allow_as_mut(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "A 'mut T' can't be created from a value of type '{}'",
                    self.fmt(expr)
                ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        node.resolved_type = if expr.is_value_type(self.db()) {
            expr
        } else {
            expr.as_mut(self.db())
        };

        node.resolved_type
    }

    fn recover_expression(
        &mut self,
        node: &mut hir::Recover,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let mut new_scope = scope.inherit(ScopeKind::Recover);
        let last_type =
            self.last_expression_type(&mut node.body, &mut new_scope);

        if last_type.is_error(self.db()) {
            return last_type;
        }

        let db = self.db();

        let result = if last_type.is_owned(db) {
            last_type.as_uni(db)
        } else if last_type.is_uni(db) {
            last_type.as_owned(db)
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "Values of type '{}' can't be recovered",
                    self.fmt(last_type)
                ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        };

        node.resolved_type = result;
        node.resolved_type
    }

    fn assign_setter(
        &mut self,
        node: &mut hir::AssignSetter,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let (receiver, allow_type_private) =
            self.call_receiver(&mut node.receiver, scope);
        let value = self.expression(&mut node.value, scope);
        let setter = node.name.name.clone() + "=";
        let module = self.module;
        let rec_id =
            if let Some(id) = self.receiver_id(receiver, &node.location) {
                id
            } else {
                return TypeRef::Error;
            };

        let method = match rec_id.lookup_method(
            self.db(),
            &setter,
            module,
            allow_type_private,
        ) {
            MethodLookup::Ok(id) => id,
            MethodLookup::Private => {
                self.private_method_call(&setter, &node.location);

                return TypeRef::Error;
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(&setter, receiver, &node.location);

                return TypeRef::Error;
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(&setter, receiver, &node.location);

                return TypeRef::Error;
            }
            MethodLookup::None => {
                return if self.assign_field_with_receiver(
                    node, receiver, rec_id, value, scope,
                ) {
                    TypeRef::nil()
                } else {
                    self.state.diagnostics.undefined_method(
                        &setter,
                        self.fmt(receiver),
                        self.file(),
                        node.location.clone(),
                    );

                    TypeRef::Error
                };
            }
        };

        let loc = &node.location;
        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.self_type, self.method)),
            receiver,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        self.positional_argument(&mut call, 0, &mut node.value, scope);
        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);

        let returns = call.return_type;

        let rec_info = Receiver::class_or_explicit(self.db(), receiver);

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn assign_field_with_receiver(
        &mut self,
        node: &mut hir::AssignSetter,
        receiver: TypeRef,
        receiver_id: TypeId,
        value: TypeRef,
        scope: &mut LexicalScope,
    ) -> bool {
        let name = &node.name.name;

        // When using `self.field = value`, none of the below is applicable, nor
        // do we need to calculate the field type as it's already cached.
        if receiver_id == self.self_type {
            return if let Some((field, typ)) = self.check_field_assignment(
                name,
                &mut node.value,
                &node.name.location,
                scope,
            ) {
                node.kind = CallKind::SetField(FieldInfo {
                    class: receiver.class_id(self.db()).unwrap(),
                    id: field,
                    variable_type: typ,
                });

                true
            } else {
                false
            };
        }

        let (ins, field) = if let TypeId::ClassInstance(ins) = receiver_id {
            if let Some(field) = ins.instance_of().field(self.db(), name) {
                (ins, field)
            } else {
                return false;
            }
        } else {
            return false;
        };

        if !field.is_visible_to(self.db(), self.module) {
            self.state.diagnostics.private_field(
                name,
                self.file(),
                node.location.clone(),
            );
        }

        if !receiver.allow_mutating() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "Can't assign a new value to field '{}', as its receiver \
                    is immutable",
                    name,
                ),
                self.module.file(self.db()),
                node.location.clone(),
            );
        }

        let targs = TypeArguments::for_class(self.db(), ins);
        let raw_type = field.value_type(self.db());
        let bounds = self.bounds;
        let var_type =
            TypeResolver::new(self.db_mut(), &targs, bounds).resolve(raw_type);

        let value = value.cast_according_to(var_type, self.db());

        if !TypeChecker::check(self.db(), value, var_type) {
            self.state.diagnostics.type_error(
                self.fmt(value),
                self.fmt(var_type),
                self.file(),
                node.location.clone(),
            );
        }

        if receiver.require_sendable_arguments(self.db())
            && !value.is_sendable(self.db())
        {
            self.state.diagnostics.unsendable_field_value(
                name,
                self.fmt(value),
                self.file(),
                node.location.clone(),
            );
        }

        node.kind = CallKind::SetField(FieldInfo {
            class: ins.instance_of(),
            id: field,
            variable_type: var_type,
        });

        true
    }

    fn call(
        &mut self,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((rec, allow_type_private)) =
            node.receiver.as_mut().map(|r| self.call_receiver(r, scope))
        {
            if let Some(closure) = rec.closure_id(self.db()) {
                self.call_closure(rec, closure, node, scope)
            } else {
                self.call_with_receiver(rec, node, scope, allow_type_private)
            }
        } else {
            self.call_without_receiver(node, scope)
        }
    }

    fn call_closure(
        &mut self,
        receiver: TypeRef,
        closure: ClosureId,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if node.name.name != CALL_METHOD {
            self.state.diagnostics.undefined_method(
                &node.name.name,
                self.fmt(receiver),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        if !receiver.allow_mutating() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                "Closures can only be called using owned or mutable references",
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        let num_given = node.arguments.len();
        let num_exp = closure.number_of_arguments(self.db());

        if num_given != num_exp {
            self.state.diagnostics.incorrect_call_arguments(
                num_given,
                num_exp,
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        let targs = TypeArguments::new();
        let mut exp_args = Vec::new();

        for (index, arg_node) in node.arguments.iter_mut().enumerate() {
            let exp = closure
                .positional_argument_input_type(self.db(), index)
                .unwrap()
                .as_rigid_type(&mut self.state.db, self.bounds);

            let arg_expr_node = match arg_node {
                hir::Argument::Positional(expr) => expr,
                hir::Argument::Named(n) => {
                    self.state.diagnostics.closure_with_named_argument(
                        self.file(),
                        n.location.clone(),
                    );

                    continue;
                }
            };

            let given = self
                .argument_expression(exp, arg_expr_node, scope, &targs)
                .cast_according_to(exp, self.db());

            if !TypeChecker::check(self.db(), given, exp) {
                self.state.diagnostics.type_error(
                    format_type(self.db(), given),
                    format_type(self.db(), exp),
                    self.file(),
                    arg_expr_node.location().clone(),
                );
            }

            exp_args.push(exp);
        }

        let returns = {
            let raw = closure.return_type(self.db());

            TypeResolver::new(&mut self.state.db, &targs, self.bounds)
                .resolve(raw)
        };

        node.kind = CallKind::CallClosure(ClosureCallInfo {
            id: closure,
            expected_arguments: exp_args,
            returns,
        });

        returns
    }

    fn call_with_receiver(
        &mut self,
        receiver: TypeRef,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
        allow_type_private: bool,
    ) -> TypeRef {
        let rec_id =
            if let Some(id) = self.receiver_id(receiver, &node.location) {
                id
            } else {
                return TypeRef::Error;
            };

        let method = match rec_id.lookup_method(
            self.db(),
            &node.name.name,
            self.module,
            allow_type_private,
        ) {
            MethodLookup::Ok(id) => id,
            MethodLookup::Private => {
                self.private_method_call(&node.name.name, &node.location);

                return TypeRef::Error;
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(
                    &node.name.name,
                    receiver,
                    &node.location,
                );

                return TypeRef::Error;
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(
                    &node.name.name,
                    receiver,
                    &node.location,
                );

                return TypeRef::Error;
            }
            MethodLookup::None if node.arguments.is_empty() => {
                if let Some(typ) =
                    self.field_with_receiver(node, receiver, rec_id, scope)
                {
                    return typ;
                }

                self.state.diagnostics.undefined_method(
                    &node.name.name,
                    self.fmt(receiver),
                    self.file(),
                    node.location.clone(),
                );

                return TypeRef::Error;
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    &node.name.name,
                    self.fmt(receiver),
                    self.file(),
                    node.location.clone(),
                );

                return TypeRef::Error;
            }
        };

        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.self_type, self.method)),
            receiver,
            rec_id,
            method,
        );

        call.check_mutability(self.state, &node.location);
        call.check_type_bounds(self.state, &node.location);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_arguments(self.state, &node.location);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);

        let returns = call.return_type;

        let rec_info = Receiver::class_or_explicit(self.db(), receiver);

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn call_without_receiver(
        &mut self,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let name = &node.name.name;
        let module = self.module;
        let rec = scope.surrounding_type;
        let rec_id = rec.type_id(self.db()).unwrap();
        let (rec_info, rec, rec_id, method) =
            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(method) => {
                    self.check_if_self_is_allowed(scope, &node.location);

                    if method.is_instance_method(self.db()) {
                        scope.mark_closures_as_capturing_self(self.db_mut());
                    }

                    let rec_info =
                        Receiver::class_or_implicit(self.db(), method);

                    (rec_info, rec, rec_id, method)
                }
                MethodLookup::Private => {
                    self.private_method_call(name, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(name, rec, &node.location);

                    return TypeRef::Error;
                }
                MethodLookup::None => {
                    if let Some(Symbol::Method(method)) =
                        self.module.symbol(self.db(), name)
                    {
                        // The receiver of imported module methods is the module
                        // they are defined in.
                        //
                        // Private module methods can't be imported, so we don't
                        // need to check the visibility here.
                        let mod_id = method.module(self.db());
                        let id = TypeId::Module(mod_id);
                        let mod_typ = TypeRef::Owned(id);

                        (
                            Receiver::Class(mod_id.class(self.db())),
                            mod_typ,
                            id,
                            method,
                        )
                    } else {
                        self.state.diagnostics.undefined_symbol(
                            name,
                            self.file(),
                            node.location.clone(),
                        );

                        return TypeRef::Error;
                    }
                }
            };

        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.self_type, self.method)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, &node.location);
        call.check_type_bounds(self.state, &node.location);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_arguments(self.state, &node.location);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, &node.location);

        let returns = call.return_type;

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn field_with_receiver(
        &mut self,
        node: &mut hir::Call,
        receiver: TypeRef,
        receiver_id: TypeId,
        scope: &mut LexicalScope,
    ) -> Option<TypeRef> {
        let name = &node.name.name;

        if receiver_id == self.self_type {
            return if let Some((field, raw_type)) = self.field_type(name) {
                let typ = self.field_reference(raw_type, scope, &node.location);

                node.kind = CallKind::GetField(FieldInfo {
                    class: receiver.class_id(self.db()).unwrap(),
                    id: field,
                    variable_type: typ,
                });

                Some(typ)
            } else {
                self.state.diagnostics.undefined_method(
                    name,
                    self.fmt(receiver),
                    self.file(),
                    node.location.clone(),
                );

                None
            };
        }

        let (ins, field) = if let TypeId::ClassInstance(ins) = receiver_id {
            ins.instance_of().field(self.db(), name).map(|field| (ins, field))
        } else {
            None
        }?;

        if !field.is_visible_to(self.db(), self.module) {
            self.state.diagnostics.private_field(
                &node.name.name,
                self.file(),
                node.location.clone(),
            );
        }

        let raw_type = field.value_type(self.db_mut());
        let immutable = receiver.is_ref(self.db_mut());
        let args = ins.type_arguments(self.db_mut()).clone();
        let bounds = self.bounds;
        let mut returns = TypeResolver::new(self.db_mut(), &args, bounds)
            .with_immutable(immutable)
            .resolve(raw_type);

        if returns.is_value_type(self.db_mut()) {
            returns = returns.as_owned(self.db_mut());
        } else if !immutable && raw_type.is_owned_or_uni(self.db_mut()) {
            returns = returns.as_mut(self.db_mut());
        }

        if receiver.require_sendable_arguments(self.db())
            && !returns.is_sendable(self.db())
        {
            self.state.diagnostics.unsendable_field(
                name,
                self.fmt(returns),
                self.file(),
                node.location.clone(),
            );
        }

        node.kind = CallKind::GetField(FieldInfo {
            id: field,
            class: ins.instance_of(),
            variable_type: returns,
        });

        Some(returns)
    }

    fn builtin_call(
        &mut self,
        node: &mut hir::BuiltinCall,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        for n in &mut node.arguments {
            self.expression(n, scope);
        }

        let id = if let Some(id) = self.db().builtin_function(&node.name.name) {
            id
        } else {
            self.state.diagnostics.undefined_symbol(
                &node.name.name,
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        };

        let returns = id.return_type();

        node.info = Some(BuiltinCallInfo { id, returns });

        returns
    }

    fn type_cast(
        &mut self,
        node: &mut hir::TypeCast,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr_type = self.expression(&mut node.value, scope);

        let rules = Rules {
            type_parameters_as_rigid: true,
            type_parameters_as_owned: true,
            ..Default::default()
        };
        let type_scope = TypeScope::with_bounds(
            self.module,
            self.self_type,
            Some(self.method),
            self.bounds,
        );

        let cast_type = DefineAndCheckTypeSignature::new(
            self.state,
            self.module,
            &type_scope,
            rules,
        )
        .define_type(&mut node.cast_to);

        // Casting to/from Any is dangerous but necessary to make the standard
        // library work.
        if !expr_type.is_any(self.db())
            && !cast_type.is_any(self.db())
            && !TypeChecker::check(self.db_mut(), expr_type, cast_type)
        {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type '{}' can't be cast to type '{}'",
                    format_type(self.db(), expr_type),
                    format_type(self.db(), cast_type)
                ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        node.resolved_type = cast_type;
        node.resolved_type
    }

    fn try_expression(
        &mut self,
        node: &mut hir::Try,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr = self.expression(&mut node.expression, scope);

        if expr.is_error(self.db()) {
            return expr;
        }

        let recovery = scope.in_recover();
        let expr_kind = expr.throw_kind(self.db());
        let ret_type = scope.return_type;
        let ret_kind = ret_type.throw_kind(self.db());

        node.return_type = ret_type;
        node.kind =
            if recovery { expr_kind.as_uni(self.db()) } else { expr_kind };

        match (expr_kind, ret_kind) {
            (ThrowKind::Option(some), ThrowKind::Option(_)) => {
                // If the value is a None, then `try` produces a new `None`, so
                // no type-checking is necessary in this case.
                return some;
            }
            (
                ThrowKind::Result(ok, expr_err),
                ThrowKind::Result(ret_ok, ret_err),
            ) => {
                if TypeChecker::check(self.db(), expr_err, ret_err) {
                    return ok;
                }

                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ret_ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location.clone(),
                );
            }
            (ThrowKind::Unknown, _) => {
                self.state.diagnostics.invalid_try(
                    format_type(self.db(), expr),
                    self.file(),
                    node.expression.location().clone(),
                );
            }
            (_, ThrowKind::Unknown) => {
                self.state
                    .diagnostics
                    .try_not_available(self.file(), node.location.clone());
            }
            (ThrowKind::Option(_), ThrowKind::Result(ret_ok, _)) => {
                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ret_ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location.clone(),
                );
            }
            (ThrowKind::Result(_, _), ThrowKind::Option(ok)) => {
                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location.clone(),
                );
            }
        }

        TypeRef::Error
    }

    fn receiver_id(
        &mut self,
        receiver: TypeRef,
        location: &SourceLocation,
    ) -> Option<TypeId> {
        match receiver.type_id(self.db()) {
            Ok(id) => Some(id),
            Err(TypeRef::Error) => None,
            Err(TypeRef::Placeholder(_)) => {
                self.state.diagnostics.cant_infer_type(
                    format_type(self.db(), receiver),
                    self.file(),
                    location.clone(),
                );

                None
            }
            Err(typ) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidCall,
                    format!(
                        "Methods can't be called on values of type '{}'",
                        self.fmt(typ)
                    ),
                    self.file(),
                    location.clone(),
                );

                None
            }
        }
    }

    fn lookup_constant(
        &mut self,
        name: &str,
        source: Option<&hir::Identifier>,
    ) -> Result<Option<Symbol>, ()> {
        if let Some(src) = source {
            if let Some(Symbol::Module(module)) =
                self.module.symbol(self.db(), &src.name)
            {
                Ok(module.symbol(self.db(), name))
            } else {
                self.state.diagnostics.symbol_not_a_module(
                    &src.name,
                    self.file(),
                    src.location.clone(),
                );

                Err(())
            }
        } else {
            Ok(self.module.symbol(self.db(), name))
        }
    }

    fn call_receiver(
        &mut self,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) -> (TypeRef, bool) {
        let typ = match node {
            hir::Expression::ConstantRef(ref mut n) => {
                self.constant(n, scope, true)
            }
            hir::Expression::IdentifierRef(ref mut n) => {
                self.identifier(n, scope, true)
            }
            _ => self.expression(node, scope),
        };

        (typ, node.is_self())
    }

    fn call_arguments(
        &mut self,
        nodes: &mut [hir::Argument],
        call: &mut MethodCall,
        scope: &mut LexicalScope,
    ) {
        for (index, arg) in nodes.iter_mut().enumerate() {
            match arg {
                hir::Argument::Positional(ref mut n) => {
                    self.positional_argument(call, index, n, scope);
                }
                hir::Argument::Named(ref mut n) => {
                    self.named_argument(call, n, scope);
                }
            }
        }
    }

    fn positional_argument(
        &mut self,
        call: &mut MethodCall,
        index: usize,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) {
        call.arguments += 1;

        if let Some(expected) =
            call.method.positional_argument_input_type(self.db(), index)
        {
            let given = self.argument_expression(
                expected,
                node,
                scope,
                &call.type_arguments,
            );

            call.check_argument(self.state, given, expected, node.location());
        } else {
            self.expression(node, scope);
        }
    }

    fn named_argument(
        &mut self,
        call: &mut MethodCall,
        node: &mut hir::NamedArgument,
        scope: &mut LexicalScope,
    ) {
        let name = &node.name.name;

        if let Some((index, expected)) =
            call.method.named_argument(self.db(), name)
        {
            // We persist the index so we don't need to look it up again when
            // lowering to MIR.
            node.index = index;

            let given = self.argument_expression(
                expected,
                &mut node.value,
                scope,
                &call.type_arguments,
            );

            if call.named_arguments.contains(name) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidCall,
                    format!(
                        "The named argument '{}' is already specified",
                        name
                    ),
                    self.file(),
                    node.name.location.clone(),
                );
            } else {
                call.named_arguments.insert(name.to_string());

                call.arguments += 1;
            }

            call.check_argument(
                self.state,
                given,
                expected,
                node.value.location(),
            );
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "The argument '{}' isn't defined by the method '{}'",
                    name,
                    call.method.name(self.db()),
                ),
                self.file(),
                node.name.location.clone(),
            );
        }
    }

    fn check_if_self_is_allowed(
        &mut self,
        scope: &LexicalScope,
        location: &SourceLocation,
    ) {
        if scope.in_closure_in_recover() {
            self.state
                .diagnostics
                .self_in_closure_in_recover(self.file(), location.clone());
        }
    }

    fn require_boolean(&mut self, typ: TypeRef, location: &SourceLocation) {
        if typ == TypeRef::Error || typ.is_bool(self.db()) {
            return;
        }

        self.state.diagnostics.error(
            DiagnosticId::InvalidType,
            format!(
                "Expected a 'Bool', 'ref Bool' or 'mut Bool', \
                found '{}' instead",
                format_type(self.db(), typ),
            ),
            self.file(),
            location.clone(),
        );
    }

    fn type_signature(
        &mut self,
        node: &mut hir::Type,
        self_type: TypeId,
    ) -> TypeRef {
        let rules = Rules {
            type_parameters_as_rigid: true,
            type_parameters_as_owned: true,
            ..Default::default()
        };
        let type_scope = TypeScope::with_bounds(
            self.module,
            self_type,
            Some(self.method),
            self.bounds,
        );

        DefineAndCheckTypeSignature::new(
            self.state,
            self.module,
            &type_scope,
            rules,
        )
        .define_type(node)
    }

    fn error_patterns(
        &mut self,
        nodes: &mut Vec<hir::Pattern>,
        pattern: &mut Pattern,
    ) {
        for node in nodes {
            self.pattern(node, TypeRef::Error, pattern);
        }
    }

    fn field_error_patterns(
        &mut self,
        nodes: &mut Vec<hir::FieldPattern>,
        pattern: &mut Pattern,
    ) {
        for node in nodes {
            self.pattern(&mut node.pattern, TypeRef::Error, pattern);
        }
    }

    fn field_type(&mut self, name: &str) -> Option<(FieldId, TypeRef)> {
        self.method.field_id_and_type(self.db(), name)
    }

    fn field_reference(
        &mut self,
        raw_type: TypeRef,
        scope: &LexicalScope,
        location: &SourceLocation,
    ) -> TypeRef {
        let typ = raw_type.cast_according_to(scope.surrounding_type, self.db());

        if scope.in_recover() && !typ.is_sendable(self.db()) {
            self.state.diagnostics.unsendable_type_in_recover(
                self.fmt(typ),
                self.file(),
                location.clone(),
            );
        }

        scope.mark_closures_as_capturing_self(self.db_mut());
        typ
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

    fn fmt(&self, typ: TypeRef) -> String {
        format_type(self.db(), typ)
    }

    fn invalid_static_call(
        &mut self,
        name: &str,
        receiver: TypeRef,
        location: &SourceLocation,
    ) {
        self.state.diagnostics.invalid_static_call(
            name,
            self.fmt(receiver),
            self.file(),
            location.clone(),
        );
    }

    fn invalid_instance_call(
        &mut self,
        name: &str,
        receiver: TypeRef,
        location: &SourceLocation,
    ) {
        self.state.diagnostics.invalid_instance_call(
            name,
            self.fmt(receiver),
            self.file(),
            location.clone(),
        );
    }

    fn private_method_call(&mut self, name: &str, location: &SourceLocation) {
        self.state.diagnostics.private_method_call(
            name,
            self.file(),
            location.clone(),
        );
    }

    fn lookup_variable(
        &mut self,
        name: &str,
        scope: &LexicalScope,
        location: &SourceLocation,
    ) -> Option<(VariableId, TypeRef, bool)> {
        let mut source = Some(scope);
        let mut crossed_uni = false;
        let mut captured = false;
        let mut allow_assignment = true;
        let db = self.db();

        // The closures that capture a variable, if any.
        //
        // Variables may be captured by nested closures, in which case we need
        // to track/pass it around accordingly for all such closures.
        let mut capturing: Vec<ClosureId> = Vec::new();

        while let Some(current) = source {
            if let Some(variable) = current.variables.variable(name) {
                let mut var_type = variable.value_type(db);

                if crossed_uni && !var_type.is_sendable(db) {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidSymbol,
                        format!(
                            "The variable '{}' exists, but its type ('{}') \
                            prohibits it from being captured by a recover \
                            expression",
                            name,
                            self.fmt(var_type)
                        ),
                        self.file(),
                        location.clone(),
                    );
                }

                if captured && var_type.is_owned_or_uni(self.db()) {
                    var_type = var_type.as_mut(self.db());
                }

                for closure in capturing {
                    closure.add_capture(self.db_mut(), variable);
                }

                // We return the variable even if it's from outside a recover
                // expression. This way we can still type-check the use of the
                // variable.
                return Some((variable, var_type, allow_assignment));
            }

            match current.kind {
                ScopeKind::Recover => crossed_uni = true,
                ScopeKind::Closure(closure) => {
                    capturing.push(closure);

                    // Captured variables are always read as references, because
                    // they are stored in the capturing closure.
                    captured = true;

                    // Captured variables can only be assigned by moving
                    // closures, as non-moving closures store references to the
                    // captured values, not the values themselves. We can't
                    // assign such captures a new value, as the value referred
                    // to (in most cases at least) wouldn't outlive the closure.
                    allow_assignment = closure.is_moving(self.db());
                }
                _ => {}
            }

            source = current.parent;
        }

        None
    }
}
