//! Passes for type-checking method body and constant expressions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{DefineAndCheckTypeSignature, Rules, TypeScope};
use location::Location;
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::swap;
use std::path::PathBuf;
use types::check::{Environment, TypeChecker};
use types::format::{format_type, format_type_with_arguments, TypeFormatter};
use types::resolve::TypeResolver;
use types::{
    Block, CallInfo, CallKind, Closure, ClosureCallInfo, ClosureId,
    ConstantKind, ConstantPatternKind, Database, FieldId, FieldInfo,
    IdentifierKind, IntrinsicCall, MethodId, MethodLookup, ModuleId, Receiver,
    Sendability, Sign, Symbol, ThrowKind, TraitId, TraitInstance,
    TypeArguments, TypeBounds, TypeEnum, TypeId, TypeInstance, TypeRef,
    Variable, VariableId, CALL_METHOD, DEREF_POINTER_FIELD, SELF_TYPE,
};

const IGNORE_VARIABLE: &str = "_";

/// The maximum number of methods that a single type can define.
///
/// We subtract 1 to account for the generated dropper methods, as these methods
/// are generated later.
const METHODS_IN_CLASS_LIMIT: usize = (u16::MAX - 1) as usize;

fn copy_inherited_type_arguments(
    db: &Database,
    source: TraitInstance,
    target: &mut TypeArguments,
) {
    let inherited = source.instance_of().inherited_type_arguments(db);

    for &param in inherited.keys() {
        // We may have an assignment chain in the form `A = B = C = X`. In such
        // a case we want A, B, and C all to resolve to X, hence the recursive
        // get.
        let arg = inherited.get_recursive(db, param).unwrap();
        let val = if let Some(id) = arg.as_type_parameter(db) {
            target.get(id).unwrap()
        } else {
            arg
        };

        target.assign(param, val);
    }
}

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
        location: Location,
    ) -> VariableId {
        let var =
            Variable::alloc(db, name.clone(), value_type, mutable, location);

        self.add_variable(name, var);
        var
    }

    fn add_variable(&mut self, name: String, variable: VariableId) {
        self.variables.insert(name, variable);
    }

    fn variable(&self, name: &str) -> Option<VariableId> {
        self.variables.get(name).cloned()
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
    check_sendable: Vec<(TypeRef, Location)>,

    /// The resolved return type of the call.
    return_type: TypeRef,
}

impl MethodCall {
    fn new(
        state: &mut State,
        module: ModuleId,
        caller: Option<(MethodId, &HashSet<TypeEnum>)>,
        receiver: TypeRef,
        receiver_id: TypeEnum,
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
        if method.is_static(&state.db) {
            if let TypeEnum::Type(typ) = receiver_id {
                if typ.is_generic(&state.db) {
                    for param in typ.type_parameters(&state.db) {
                        type_arguments.assign(
                            param,
                            TypeRef::placeholder(&mut state.db, Some(param)),
                        );
                    }
                }
            }
        }

        // When calling a method on a trait or a type parameter, the method may
        // end up referring to a type parameter from a parent trait. We need to
        // make sure those type parameters are mapped to the correct final
        // values, so we have to expose them to the call's type arguments.
        match receiver_id {
            TypeEnum::TraitInstance(ins) => copy_inherited_type_arguments(
                &state.db,
                ins,
                &mut type_arguments,
            ),
            TypeEnum::TypeParameter(id) | TypeEnum::RigidTypeParameter(id) => {
                for ins in id.requirements(&state.db) {
                    copy_inherited_type_arguments(
                        &state.db,
                        ins,
                        &mut type_arguments,
                    );
                }
            }
            _ => {}
        }

        // When a method is implemented through a trait, it may depend on type
        // parameters of that trait. To ensure these are mapped to the final
        // inferred types, we have to copy them over into our temporary type
        // arguments.
        if let Some(ins) = method.implemented_trait_instance(&state.db) {
            ins.copy_type_arguments_into(&state.db, &mut type_arguments);
        }

        let require_sendable = receiver.require_sendable_arguments(&state.db)
            && !method.is_moving(&state.db);

        let rec_is_rigid = receiver.is_rigid_type_parameter(&state.db);
        let bounds = if let Some((caller, self_types)) = caller {
            // If the receiver is `self`, a field from `self`, or a type
            // parameter that originates from a field in `self` (in which case
            // it's rigid), we need to take the bounds of the surrounding method
            // into account.
            if self_types.contains(&receiver_id) || rec_is_rigid {
                // The bounds of the surrounding method need to be exposed as
                // type arguments, such that if we return a bounded parameter
                // from some deeply nested type (e.g. a type parameter
                // requirement), we still remap it correctly.
                for (&k, &v) in caller.bounds(&state.db).iter() {
                    type_arguments.assign(
                        k,
                        TypeRef::Any(TypeEnum::RigidTypeParameter(v)),
                    );
                }

                caller.bounds(&state.db).union(method.bounds(&state.db))
            } else {
                method.bounds(&state.db).clone()
            }
        } else {
            method.bounds(&state.db).clone()
        };

        // If the receiver is rigid, it may introduce additional type arguments
        // through its type parameter requirements. We need to ensure that these
        // are all returned as rigid parameters as well. In addition, we need to
        // take care or remapping any bound parameters.
        //
        // We don't do this ahead of time (e.g. when defining the type
        // parameters), as that would involve copying lots of data structures,
        // and because it complicates looking up type parameters, so instead we
        // handle it here when/if necessary.
        if rec_is_rigid {
            for val in type_arguments.values_mut() {
                *val = match val {
                    TypeRef::Any(TypeEnum::TypeParameter(id)) => {
                        TypeRef::Any(TypeEnum::RigidTypeParameter(
                            bounds.get(*id).unwrap_or(*id),
                        ))
                    }
                    TypeRef::Owned(TypeEnum::TypeParameter(id)) => {
                        TypeRef::Owned(TypeEnum::RigidTypeParameter(
                            bounds.get(*id).unwrap_or(*id),
                        ))
                    }
                    _ => *val,
                };
            }
        }

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

    fn check_type_bounds(&mut self, state: &mut State, location: Location) {
        let args = self.type_arguments.clone();
        let mut scope = Environment::new(args.clone(), args);
        let mut checker = TypeChecker::new(&state.db);

        if !checker.check_bounds(&self.bounds, &mut scope) {
            state.diagnostics.error(
                DiagnosticId::InvalidSymbol,
                format!(
                    "the method '{}' exists but isn't available because \
                    one or more type parameter bounds aren't met",
                    self.method.name(&state.db),
                ),
                self.module.file(&state.db),
                location,
            );
        }
    }

    fn check_arguments(&mut self, state: &mut State, location: Location) {
        let expected = self.method.number_of_arguments(&state.db);

        if self.arguments > expected && self.method.is_variadic(&state.db) {
            return;
        }

        if self.arguments != expected {
            state.diagnostics.incorrect_call_arguments(
                self.arguments,
                expected,
                self.module.file(&state.db),
                location,
            );
        }
    }

    fn check_mutability(&mut self, state: &mut State, location: Location) {
        let name = self.method.name(&state.db);
        let rec = self.receiver;

        if self.method.is_moving(&state.db) && !rec.allow_moving(&state.db) {
            state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "the method '{}' takes ownership of its receiver, \
                    but '{}' isn't an owned value",
                    name,
                    format_type_with_arguments(
                        &state.db,
                        &self.type_arguments,
                        rec
                    )
                ),
                self.module.file(&state.db),
                location,
            );

            return;
        }

        if self.method.is_mutable(&state.db) && !rec.allow_mutating(&state.db) {
            state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "the method '{}' requires a mutable receiver, \
                    but '{}' isn't mutable",
                    name,
                    format_type_with_arguments(
                        &state.db,
                        &self.type_arguments,
                        rec
                    )
                ),
                self.module.file(&state.db),
                location,
            );
        }
    }

    /// Checks if an argument is compatible with the expected argument type.
    ///
    /// The return type is the resolved _expected_ type.
    fn check_argument(
        &mut self,
        state: &mut State,
        argument: TypeRef,
        expected: TypeRef,
        location: Location,
    ) -> TypeRef {
        let given = argument.cast_according_to(&state.db, expected);

        if self.require_sendable || given.is_uni_value_borrow(&state.db) {
            self.check_sendable.push((given, location));
        }

        let rec = self.receiver.as_type_enum(&state.db).unwrap();
        let mut env = Environment::with_right_self_type(
            given.type_arguments(&state.db),
            self.type_arguments.clone(),
            rec,
        );

        if !TypeChecker::new(&state.db)
            .check_argument(given, expected, &mut env)
        {
            let rhs =
                TypeFormatter::with_self_type(&state.db, rec, Some(&env.right))
                    .format(expected);

            state.diagnostics.type_error(
                format_type_with_arguments(&state.db, &env.left, given),
                rhs,
                self.module.file(&state.db),
                location,
            );
        }

        TypeResolver::new(&mut state.db, &env.right, &self.bounds)
            .with_self_type(rec)
            .resolve(expected)
    }

    fn check_sendable(
        &mut self,
        state: &mut State,
        usage: hir::Usage,
        location: Location,
    ) {
        if !self.require_sendable {
            return;
        }

        let immutable = self.method.is_immutable(&state.db);
        let sendable_rec =
            self.receiver.as_owned(&state.db).is_sendable_output(&state.db);
        let maybe_allow_borrows = immutable || sendable_rec;
        let mut allow_borrows = maybe_allow_borrows;
        let mut args = Vec::with_capacity(self.check_sendable.len());

        for (typ, _) in &self.check_sendable {
            let send = typ.sendability(&state.db, maybe_allow_borrows);

            if matches!(send, Sendability::NotSendable) {
                allow_borrows = false;
            }

            args.push(send);
        }

        for (&(given, loc), send) in self.check_sendable.iter().zip(args) {
            match send {
                Sendability::Sendable => continue,
                Sendability::SendableRef | Sendability::SendableMut
                    if allow_borrows =>
                {
                    continue;
                }
                _ => {}
            }

            let targs = &self.type_arguments;

            state.diagnostics.unsendable_argument(
                format_type_with_arguments(&state.db, targs, given),
                self.module.file(&state.db),
                loc,
            );
        }

        // If the return value is unused then it doesn't matter whether it's
        // sendable or not.
        if !usage.is_used() {
            return;
        }

        // In certain cases it's fine to allow non-unique owned values to be
        // returned, provided we can guarantee (based on what we know at the
        // call site) no uniqueness constaints are violated.
        //
        // For immutable methods, if all the arguments are sendable then
        // returned owned values can't be aliased by the callee.
        //
        // For mutable methods, we additionally require that the receiver can't
        // ever store aliases to the returned data. Since the receiver is likely
        // typed as `uni T` (which itself is sendable) we perform that check
        // against its owned counterpart.
        let ret_sendable = if allow_borrows {
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
                location,
            );
        }
    }

    fn resolve_return_type(&mut self, state: &mut State) -> TypeRef {
        let raw = self.method.return_type(&state.db);
        let rigid = self.receiver.is_rigid_type_parameter(&state.db);
        let rec = self.receiver.as_type_enum(&state.db).unwrap();

        self.return_type = TypeResolver::new(
            &mut state.db,
            &self.type_arguments,
            &self.bounds,
        )
        .with_rigid(rigid)
        .with_owned()
        .with_self_type(rec)
        .resolve(raw);

        self.return_type
    }
}

/// A compiler pass for defining the types of constants.
pub(crate) fn define_constants(
    state: &mut State,
    modules: &mut [hir::Module],
) -> bool {
    // We use a work list such that we can handle constants defined and referred
    // to in any order, as well as nested dependencies (e.g. `A = B = C = D`).
    //
    // We need explicit type annotations here due to
    // https://github.com/rust-lang/rust/issues/129694.
    let mut work: VecDeque<(ModuleId, &mut hir::DefineConstant)> =
        VecDeque::new();
    let mut new_work: VecDeque<(ModuleId, &mut hir::DefineConstant)> =
        VecDeque::new();

    for module in modules.iter_mut() {
        for expr in &mut module.expressions {
            if let hir::TopLevelExpression::Constant(ref mut n) = expr {
                work.push_back((module.module_id, n));
            }
        }
    }

    while !work.is_empty() {
        // This flag is used to track if _any_ constant in the stack is resolved
        // to a type. If this isn't the case, we produce an error.
        let mut resolved = false;

        while let Some((mid, node)) = work.pop_front() {
            let id = node.constant_id.unwrap();

            match CheckConstant::new(state, mid).expression(&mut node.value) {
                // The type will be unknown if our constant depends on one or
                // more other constants that we have yet to process.
                TypeRef::Unknown => {
                    new_work.push_back((mid, node));
                }
                typ => {
                    id.set_value_type(&mut state.db, typ);
                    resolved = true;
                }
            }
        }

        swap(&mut work, &mut new_work);

        // If we're unable to determine the type for _any_ of the constants,
        // it's due to a circular dependency (e.g. `A = B` and `B = A`). In
        // this case there's nothing we can do other than produce an error.
        if resolved {
            continue;
        }

        for (module, node) in work.drain(0..) {
            state.diagnostics.error(
                DiagnosticId::InvalidType,
                "the type of this constant can't be inferred",
                module.file(&state.db),
                node.name.location,
            );
        }
    }

    !state.diagnostics.has_errors()
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
                hir::TopLevelExpression::Type(ref mut n) => {
                    self.define_type(n);
                }
                hir::TopLevelExpression::Trait(ref mut n) => {
                    self.define_trait(n);
                }
                hir::TopLevelExpression::Reopen(ref mut n) => {
                    self.reopen_type(n);
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

    fn define_type(&mut self, node: &mut hir::DefineType) {
        let id = node.type_id.unwrap();
        let num_methods = id.number_of_methods(self.db());

        if num_methods > METHODS_IN_CLASS_LIMIT {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "the number of methods defined in this type ({}) \
                    exceeds the maximum of {} methods",
                    num_methods, METHODS_IN_CLASS_LIMIT
                ),
                self.module.file(self.db()),
                node.location,
            );
        }

        self.verify_type_parameter_requirements(&node.type_parameters);

        for node in &mut node.body {
            match node {
                hir::TypeExpression::AsyncMethod(ref mut n) => {
                    self.define_async_method(n);
                }
                hir::TypeExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(n);
                }
                hir::TypeExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(n);
                }
                _ => {}
            }
        }
    }

    fn reopen_type(&mut self, node: &mut hir::ReopenType) {
        for node in &mut node.body {
            match node {
                hir::ReopenTypeExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(n)
                }
                hir::ReopenTypeExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(n)
                }
                hir::ReopenTypeExpression::AsyncMethod(ref mut n) => {
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

        checker.method_body(returns, &mut node.body, &mut scope, node.location);
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

        checker.method_body(returns, &mut node.body, &mut scope, node.location);
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

        checker.method_body(returns, &mut node.body, &mut scope, node.location);
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

        checker.method_body(returns, &mut node.body, &mut scope, node.location);
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
                        "the traits '{}' and '{}' both define a \
                        method with the same name",
                        format_type(self.db(), id),
                        format_type(self.db(), req_id),
                    ),
                    self.module.file(self.db()),
                    req.location,
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
            hir::ConstExpression::True(ref mut n) => self.true_literal(n),
            hir::ConstExpression::False(ref mut n) => self.false_literal(n),
            hir::ConstExpression::Binary(ref mut n) => self.binary(n),
            hir::ConstExpression::ConstantRef(ref mut n) => self.constant(n),
            hir::ConstExpression::Array(ref mut n) => self.array(n),
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
        node.resolved_type = TypeRef::string();
        node.resolved_type
    }

    fn true_literal(&mut self, node: &mut hir::True) -> TypeRef {
        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn false_literal(&mut self, node: &mut hir::False) -> TypeRef {
        node.resolved_type = TypeRef::boolean();
        node.resolved_type
    }

    fn binary(&mut self, node: &mut hir::ConstBinary) -> TypeRef {
        #[allow(clippy::needless_match)]
        let left = match self.expression(&mut node.left) {
            TypeRef::Unknown => return TypeRef::Unknown,
            typ => typ,
        };

        #[allow(clippy::needless_match)]
        let right = match self.expression(&mut node.right) {
            TypeRef::Unknown => return TypeRef::Unknown,
            typ => typ,
        };
        let name = node.operator.method_name();
        let (left_id, method) = if let Some(found) =
            self.lookup_method(left, name, node.location)
        {
            found
        } else {
            return TypeRef::Error;
        };

        let loc = node.location;
        let mut call = MethodCall::new(
            self.state,
            self.module,
            None,
            left,
            left_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        call.arguments = 1;

        if let Some(expected) =
            call.method.positional_argument_input_type(self.db(), 0)
        {
            call.check_argument(
                self.state,
                right,
                expected,
                node.right.location(),
            );
        }

        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, hir::Usage::Used, loc);

        node.resolved_type = call.return_type;
        node.resolved_type
    }

    fn constant(&mut self, node: &mut hir::ConstantRef) -> TypeRef {
        let name = &node.name;
        let symbol = if let Some(src) = node.source.as_ref() {
            if let Some(Symbol::Module(module)) =
                self.module.use_symbol(self.db_mut(), &src.name)
            {
                module.use_symbol(self.db_mut(), name)
            } else {
                self.state.diagnostics.symbol_not_a_module(
                    &src.name,
                    self.file(),
                    src.location,
                );

                return TypeRef::Error;
            }
        } else {
            self.module.use_symbol(self.db_mut(), name)
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
                    node.location,
                );

                TypeRef::Error
            }
            _ => {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    node.location,
                );

                TypeRef::Error
            }
        }
    }

    fn array(&mut self, node: &mut hir::ConstArray) -> TypeRef {
        let mut types = Vec::with_capacity(node.values.len());

        for n in &mut node.values {
            match self.expression(n) {
                TypeRef::Unknown => return TypeRef::Unknown,
                typ => types.push(typ),
            }
        }

        if types.len() > 1 {
            let &first = types.first().unwrap();

            for (&typ, node) in types[1..].iter().zip(node.values[1..].iter()) {
                if !TypeChecker::check(self.db(), typ, first) {
                    self.state.diagnostics.type_error(
                        format_type(self.db(), typ),
                        format_type(self.db(), first),
                        self.file(),
                        node.location(),
                    );
                }
            }
        }

        // Mutating constant arrays isn't safe, so they're typed as `ref
        // Array[T]` instead of `Array[T]`.
        let ary = TypeRef::Ref(TypeEnum::TypeInstance(
            TypeInstance::with_types(self.db_mut(), TypeId::array(), types),
        ));

        node.resolved_type = ary;
        node.resolved_type
    }

    fn lookup_method(
        &mut self,
        receiver: TypeRef,
        name: &str,
        location: Location,
    ) -> Option<(TypeEnum, MethodId)> {
        let rec_id = match receiver.as_type_enum(self.db()) {
            Ok(id) => id,
            Err(TypeRef::Error) => return None,
            Err(typ) => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type(self.db(), typ),
                    self.file(),
                    location,
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
                    location,
                );
            }
            MethodLookup::InstanceOnStatic => {
                self.state.diagnostics.invalid_instance_call(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location,
                );
            }
            MethodLookup::StaticOnInstance => {
                self.state.diagnostics.invalid_static_call(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location,
                );
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type(self.db(), receiver),
                    self.file(),
                    location,
                );
            }
        }

        None
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

struct ExpectedClosure<'a> {
    /// The type ID of the closure that is expected.
    id: ClosureId,

    /// The full type of the expected closure
    value_type: TypeRef,

    /// The type arguments to expose when resolving types.
    arguments: &'a TypeArguments,

    /// The type to replace `Self` with.
    self_type: TypeEnum,
}

/// A visitor for type-checking the bodies of methods.
struct CheckMethodBody<'a> {
    state: &'a mut State,

    /// The module the method is defined in.
    module: ModuleId,

    /// The surrounding method.
    method: MethodId,

    /// The type ID of the receiver of the surrounding method.
    self_type: TypeEnum,

    /// Any bounds to apply to type parameters.
    bounds: &'a TypeBounds,

    /// The type IDs that are or originate from `self`.
    self_types: HashSet<TypeEnum>,
}

impl<'a> CheckMethodBody<'a> {
    fn new(
        state: &'a mut State,
        module: ModuleId,
        method: MethodId,
        self_type: TypeEnum,
        bounds: &'a TypeBounds,
    ) -> Self {
        let mut self_types: HashSet<TypeEnum> = method
            .fields(&state.db)
            .into_iter()
            .filter_map(|(_, typ)| typ.as_type_enum(&state.db).ok())
            .collect();

        self_types.insert(self_type);
        Self { state, module, method, self_type, bounds, self_types }
    }

    fn expressions(
        &mut self,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
    ) -> Vec<TypeRef> {
        let mut types = Vec::with_capacity(nodes.len());
        let max = nodes.len().saturating_sub(1);

        for (idx, node) in nodes.iter_mut().enumerate() {
            let usage = if idx == max
                && matches!(scope.kind, ScopeKind::Method)
                && scope.return_type.is_nil(self.db())
            {
                hir::Usage::Discarded
            } else if idx < max {
                hir::Usage::Unused
            } else {
                hir::Usage::Used
            };

            node.set_usage(usage);
            types.push(self.expression(node, scope));
        }

        types
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

    fn method_body(
        &mut self,
        returns: TypeRef,
        nodes: &mut [hir::Expression],
        scope: &mut LexicalScope,
        fallback_location: Location,
    ) {
        let typ = self.last_expression_type(nodes, scope);

        if returns.is_nil(self.db()) {
            // When the return type is `Nil` (explicit or not), we just ignore
            // whatever value is returned.
            return;
        }

        if !TypeChecker::check_return(self.db(), typ, returns, self.self_type) {
            let loc =
                nodes.last().map(|n| n.location()).unwrap_or(fallback_location);

            self.state.diagnostics.type_error(
                format_type(self.db(), typ),
                format_type(self.db(), returns),
                self.file(),
                loc,
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
            hir::Expression::AssignField(ref mut n) => {
                self.assign_field(n, scope)
            }
            hir::Expression::ReplaceField(ref mut n) => {
                self.replace_field(n, scope)
            }
            hir::Expression::AssignSetter(ref mut n) => {
                self.assign_setter(n, scope)
            }
            hir::Expression::ReplaceSetter(ref mut n) => {
                self.replace_setter(n, scope)
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
            hir::Expression::Call(ref mut n) => self.call(n, scope, false),
            hir::Expression::Closure(ref mut n) => self.closure(n, None, scope),
            hir::Expression::ConstantRef(ref mut n) => {
                self.constant(n, scope, false)
            }
            hir::Expression::DefineVariable(ref mut n) => {
                self.define_variable(n, scope)
            }
            hir::Expression::False(ref mut n) => self.false_literal(n),
            hir::Expression::FieldRef(ref mut n) => self.field(n, scope),
            hir::Expression::Float(ref mut n) => self.float_literal(n),
            hir::Expression::IdentifierRef(ref mut n) => {
                self.identifier(n, scope, false)
            }
            hir::Expression::Int(ref mut n) => self.int_literal(n),
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
            hir::Expression::String(ref mut n) => self.string_literal(n),
            hir::Expression::Throw(ref mut n) => {
                self.throw_expression(n, scope)
            }
            hir::Expression::True(ref mut n) => self.true_literal(n),
            hir::Expression::Nil(ref mut n) => self.nil_literal(n),
            hir::Expression::Tuple(ref mut n) => self.tuple_literal(n, scope),
            hir::Expression::TypeCast(ref mut n) => self.type_cast(n, scope),
            hir::Expression::Try(ref mut n) => self.try_expression(n, scope),
            hir::Expression::SizeOf(ref mut n) => self.size_of(n),
        }
    }

    fn input_expression(
        &mut self,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let typ = self.expression(node, scope);

        if typ.is_uni_value(self.db()) {
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
        node: &mut hir::Expression,
        receiver_type: TypeEnum,
        expected_type: TypeRef,
        type_arguments: &TypeArguments,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        match node {
            hir::Expression::Closure(ref mut n) => {
                let expected = expected_type.closure_id(self.db()).map(|id| {
                    ExpectedClosure {
                        id,
                        value_type: expected_type,
                        arguments: type_arguments,
                        self_type: receiver_type,
                    }
                });

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

    fn int_literal(&mut self, node: &mut hir::IntLiteral) -> TypeRef {
        node.resolved_type = TypeRef::int();
        node.resolved_type
    }

    fn float_literal(&mut self, node: &mut hir::FloatLiteral) -> TypeRef {
        node.resolved_type = TypeRef::float();
        node.resolved_type
    }

    fn string_literal(&mut self, node: &mut hir::StringLiteral) -> TypeRef {
        node.resolved_type = TypeRef::string();
        node.resolved_type
    }

    fn tuple_literal(
        &mut self,
        node: &mut hir::TupleLiteral,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let types = self.input_expressions(&mut node.values, scope);
        let typ = if let Some(id) = TypeId::tuple(types.len()) {
            id
        } else {
            self.state.diagnostics.tuple_size_error(self.file(), node.location);

            return TypeRef::Error;
        };

        let tuple = TypeRef::Owned(TypeEnum::TypeInstance(
            TypeInstance::with_types(self.db_mut(), typ, types.clone()),
        ));

        node.type_id = Some(typ);
        node.resolved_type = tuple;
        node.value_types = types;
        node.resolved_type
    }

    fn self_expression(
        &mut self,
        node: &mut hir::SelfObject,
        scope: &LexicalScope,
    ) -> TypeRef {
        let mut typ = scope.surrounding_type;

        if !self.method.is_instance(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidSymbol,
                "'self' can only be used in instance methods",
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        // Closures inside a `recover` can't refer to `self`, because they can't
        // capture `uni ref T` / `uni mut T` values.
        self.check_if_self_is_allowed(scope, node.location);

        if scope.in_recover() {
            typ = typ.as_uni_borrow(self.db());
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
        let discard = node.name.name == IGNORE_VARIABLE;

        if discard {
            node.value.set_usage(hir::Usage::Discarded);
        }

        let value_type = self.input_expression(&mut node.value, scope);

        if !value_type.is_assignable(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "values of type '{}' can't be assigned to variables",
                    format_type(self.db(), value_type)
                ),
                self.file(),
                node.value.location(),
            );
        }

        let var_type = if let Some(tnode) = node.value_type.as_mut() {
            let exp_type = self.type_signature(tnode, self.self_type);
            let value_casted =
                value_type.cast_according_to(self.db(), exp_type);

            if !TypeChecker::check(self.db(), value_casted, exp_type) {
                self.state.diagnostics.type_error(
                    format_type(self.db(), value_type),
                    format_type(self.db(), exp_type),
                    self.file(),
                    node.value.location(),
                );
            }

            exp_type
        } else {
            value_type
        };

        let name = &node.name.name;
        let rtype = TypeRef::nil();

        node.resolved_type = var_type;

        if discard {
            return rtype;
        }

        let id = scope.variables.new_variable(
            self.db_mut(),
            name.clone(),
            var_type,
            node.mutable,
            node.name.location,
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
            hir::Pattern::Type(ref mut n) => {
                self.type_pattern(n, value_type, pattern);
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
            hir::Pattern::Constructor(ref mut n) => {
                self.constructor_pattern(n, value_type, pattern);
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
                    node.location,
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
                node.location,
            );
        }

        if let Some(existing) = pattern.variable_scope.variable(&name) {
            let ex_type = existing.value_type(self.db());

            if !TypeChecker::check(self.db(), var_type, ex_type) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "the type of this variable is defined as '{}' \
                        in another pattern, but here its type is '{}'",
                        format_type(self.db(), ex_type),
                        format_type(self.db(), var_type),
                    ),
                    self.file(),
                    node.location,
                );
            }

            if existing.is_mutable(self.db()) != node.mutable {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidPattern,
                    "the mutability of this binding must be the same \
                    in all patterns",
                    self.file(),
                    node.location,
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
            node.name.location,
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
            let constructor = if let Some(v) =
                ins.instance_of().constructor(self.db(), name)
            {
                v
            } else {
                self.state.diagnostics.undefined_constructor(
                    name,
                    format_type(self.db(), value_type),
                    self.file(),
                    node.location,
                );

                return;
            };

            let members = constructor.arguments(self.db());

            if !members.is_empty() {
                self.state.diagnostics.incorrect_pattern_arguments(
                    0,
                    members.len(),
                    self.file(),
                    node.location,
                );

                return;
            }

            node.kind = ConstantPatternKind::Constructor(constructor);

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
                            "expected a 'String' or 'Int', found '{}' instead",
                            format_type(self.db(), typ),
                        ),
                        self.file(),
                        node.location,
                    );

                    return;
                };

                typ
            }
            Ok(Some(_)) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!("the symbol '{}' is not a constant", name),
                    self.file(),
                    node.location,
                );

                return;
            }
            Ok(None) => {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    node.location,
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
                node.location,
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
            TypeRef::Owned(TypeEnum::TypeInstance(ins))
            | TypeRef::Ref(TypeEnum::TypeInstance(ins))
            | TypeRef::Mut(TypeEnum::TypeInstance(ins))
            | TypeRef::Uni(TypeEnum::TypeInstance(ins))
                if ins.instance_of().kind(self.db()).is_tuple() =>
            {
                ins
            }
            _ => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "this pattern expects a tuple, \
                        but the input type is '{}'",
                        format_type(self.db(), value_type),
                    ),
                    self.file(),
                    node.location,
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
                    "this pattern requires {} tuple members, \
                    but the input has {} members",
                    params,
                    node.values.len()
                ),
                self.file(),
                node.location,
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let raw_types = ins.ordered_type_arguments(self.db());
        let mut values = Vec::with_capacity(raw_types.len());
        let fields = ins.instance_of().fields(self.db());

        for (patt, vtype) in node.values.iter_mut().zip(raw_types.into_iter()) {
            let typ = vtype.cast_according_to(self.db(), value_type);

            self.pattern(patt, typ, pattern);
            values.push(typ);
        }

        node.field_ids = fields;
    }

    fn type_pattern(
        &mut self,
        node: &mut hir::TypePattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        if value_type == TypeRef::Error {
            self.field_error_patterns(&mut node.values, pattern);
            return;
        }

        let Some(ins) =
            value_type.as_type_instance_for_pattern_matching(self.db())
        else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "this pattern can't be used with values of type '{}'",
                    format_type(self.db(), value_type),
                ),
                self.file(),
                node.location,
            );

            self.field_error_patterns(&mut node.values, pattern);
            return;
        };

        let typ = ins.instance_of();

        if typ.has_destructor(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "the type '{}' can't be destructured as it defines \
                    a custom destructor",
                    format_type(self.db(), value_type)
                ),
                self.file(),
                node.location,
            );
        }

        if typ.kind(self.db()).is_enum() {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                "enum types don't support type patterns",
                self.file(),
                node.location,
            );
        }

        let immutable = value_type.is_ref(self.db());
        let args = TypeArguments::for_type(self.db(), ins);

        for node in &mut node.values {
            let name = &node.field.name;
            let field = if let Some(f) = typ.field(self.db(), name) {
                f
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "the type '{}' doesn't define the field '{}'",
                        format_type(self.db(), value_type),
                        name
                    ),
                    self.file(),
                    node.field.location,
                );

                self.pattern(&mut node.pattern, TypeRef::Error, pattern);
                continue;
            };

            let raw_type = field.value_type(self.db());
            let field_type =
                TypeResolver::new(&mut self.state.db, &args, self.bounds)
                    .with_immutable(immutable)
                    .resolve(raw_type)
                    .cast_according_to(self.db(), value_type);

            node.field_id = Some(field);

            self.pattern(&mut node.pattern, field_type, pattern);
        }

        node.type_id = Some(typ);
    }

    fn int_pattern(&mut self, node: &mut hir::IntLiteral, input_type: TypeRef) {
        self.expression_pattern(TypeRef::int(), input_type, node.location);
    }

    fn string_pattern(
        &mut self,
        node: &mut hir::StringPattern,
        input_type: TypeRef,
    ) {
        let typ = TypeRef::string();

        self.expression_pattern(typ, input_type, node.location);
    }

    fn true_pattern(&mut self, node: &mut hir::True, input_type: TypeRef) {
        let typ = TypeRef::boolean();

        self.expression_pattern(typ, input_type, node.location);
    }

    fn false_pattern(&mut self, node: &mut hir::False, input_type: TypeRef) {
        let typ = TypeRef::boolean();

        self.expression_pattern(typ, input_type, node.location);
    }

    fn expression_pattern(
        &mut self,
        pattern_type: TypeRef,
        input_type: TypeRef,
        location: Location,
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
                    "the type of this pattern is '{}', \
                    but the input type is '{}'",
                    format_type(self.db(), pattern_type),
                    format_type(self.db(), input_type),
                ),
                self.file(),
                location,
            );
        }
    }

    fn constructor_pattern(
        &mut self,
        node: &mut hir::ConstructorPattern,
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
                    "this pattern expects an enum type, \
                    but the input type is '{}'",
                    format_type(self.db(), value_type),
                ),
                self.file(),
                node.location,
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        };

        let name = &node.name.name;
        let typ = ins.instance_of();

        let constructor = if let Some(v) = typ.constructor(self.db(), name) {
            v
        } else {
            self.state.diagnostics.undefined_constructor(
                name,
                format_type(self.db(), value_type),
                self.file(),
                node.location,
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        };

        let members = constructor.arguments(self.db()).to_vec();

        if members.len() != node.values.len() {
            self.state.diagnostics.incorrect_pattern_arguments(
                node.values.len(),
                members.len(),
                self.file(),
                node.location,
            );

            self.error_patterns(&mut node.values, pattern);
            return;
        }

        let immutable = value_type.is_ref(self.db());
        let args = TypeArguments::for_type(self.db(), ins);
        let bounds = self.bounds;

        for (patt, member) in node.values.iter_mut().zip(members.into_iter()) {
            let typ = TypeResolver::new(self.db_mut(), &args, bounds)
                .with_immutable(immutable)
                .resolve(member)
                .cast_according_to(self.db(), value_type);

            self.pattern(patt, typ, pattern);
        }

        node.constructor_id = Some(constructor);
    }

    fn or_pattern(
        &mut self,
        node: &mut hir::OrPattern,
        value_type: TypeRef,
        pattern: &mut Pattern,
    ) {
        let mut patterns = Vec::new();
        let mut all_vars = Vec::new();
        let mut unreachable = false;

        for node in node.patterns.iter_mut() {
            // Patterns such as `a or a` are rare and likely unintentional. As
            // the pattern matching compiler handles this fine, we emit a
            // warning instead of an error.
            if unreachable {
                self.state
                    .diagnostics
                    .unreachable_pattern(self.file(), node.location());
            } else if matches!(
                node,
                hir::Pattern::Wildcard(_) | hir::Pattern::Identifier(_)
            ) {
                unreachable = true;
            }

            let mut new_pattern = Pattern::new(pattern.variable_scope);

            self.pattern(node, value_type, &mut new_pattern);
            all_vars.extend(new_pattern.variables.keys().cloned());
            patterns.push((new_pattern.variables, node.location()));
        }

        // Now that all patterns have defined their variables, we can check
        // each pattern to ensure they all define the same variables. This
        // is needed as code like `case A(a), B(b) -> test(a)` is invalid,
        // as the variable could be undefined depending on which pattern
        // matched.
        for (vars, location) in &patterns {
            for name in &all_vars {
                if vars.contains_key(name) {
                    continue;
                }

                self.state.diagnostics.error(
                    DiagnosticId::InvalidPattern,
                    format!("this pattern must define the variable '{}'", name),
                    self.file(),
                    *location,
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
            node.variable.location,
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
            node.variable.location,
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
        location: Location,
        scope: &mut LexicalScope,
    ) -> Option<(VariableId, TypeRef)> {
        let (var, _, allow_assignment) =
            if let Some(val) = self.lookup_variable(name, scope, location) {
                val
            } else {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    location,
                );

                return None;
            };

        if !allow_assignment {
            self.state.diagnostics.error(
                DiagnosticId::InvalidAssign,
                "variables captured by non-moving closures can't be assigned \
                new values"
                    .to_string(),
                self.file(),
                location,
            );

            return None;
        }

        if !var.is_mutable(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidAssign,
                format!(
                    "the variable '{}' is immutable and can't be \
                    assigned a new value",
                    name
                ),
                self.file(),
                location,
            );

            return None;
        }

        let val_type = self.expression(value_node, scope);
        let var_type = var.value_type(self.db());

        if !TypeChecker::check(self.db(), val_type, var_type) {
            self.state.diagnostics.type_error(
                format_type(self.db(), val_type),
                format_type(self.db(), var_type),
                self.file(),
                location,
            );

            return None;
        }

        Some((var, var_type))
    }

    fn closure(
        &mut self,
        node: &mut hir::Closure,
        mut expected: Option<ExpectedClosure>,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let self_type = self.self_type;
        let moving = node.moving
            || expected.as_ref().map_or(false, |e| e.id.is_moving(self.db()));

        let closure = Closure::alloc(self.db_mut(), moving);
        let bounds = self.bounds;
        let return_type = if let Some(n) = node.return_type.as_mut() {
            self.type_signature(n, self_type)
        } else {
            let db = self.db_mut();

            expected
                .as_mut()
                .map(|e| {
                    let raw = e.id.return_type(db);

                    TypeResolver::new(db, e.arguments, bounds)
                        .with_self_type(e.self_type)
                        .resolve(raw)
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

        for (idx, arg) in node.arguments.iter_mut().enumerate() {
            let name = arg.name.name.clone();
            let typ = if let Some(n) = arg.value_type.as_mut() {
                self.type_signature(n, self.self_type)
            } else {
                let db = self.db_mut();

                expected
                    .as_mut()
                    .and_then(|e| {
                        e.id.positional_argument_input_type(db, idx).map(|t| {
                            TypeResolver::new(db, e.arguments, bounds)
                                .with_self_type(e.self_type)
                                .resolve(t)
                        })
                    })
                    .unwrap_or_else(|| TypeRef::placeholder(db, None))
            };

            let var = closure.new_argument(
                self.db_mut(),
                name.clone(),
                typ,
                typ,
                arg.location,
            );

            new_scope.variables.add_variable(name, var);
        }

        self.method_body(
            return_type,
            &mut node.body,
            &mut new_scope,
            node.location,
        );

        node.resolved_type = match expected.as_ref() {
            // If a closure is immediately passed to a `uni fn`, and we don't
            // capture any variables, we can safely infer the closure as unique.
            // This removes the need for `recover fn { ... }` in most cases
            // where a `uni fn` is needed.
            //
            // `fn move` closures are not inferred as `uni fn`, as the values
            // moved into the closure may still be referred to from elsewhere.
            Some(exp)
                if exp.value_type.is_uni_value(self.db())
                    && closure.can_infer_as_uni(self.db()) =>
            {
                TypeRef::Uni(TypeEnum::Closure(closure))
            }
            _ => TypeRef::Owned(TypeEnum::Closure(closure)),
        };

        // We can't allow capturing of 'self' borrows in default methods as for
        // inline types it could result in mutations taking place on a copy when
        // the user expects the original value to be mutated instead.
        if let Some(stype) = closure.captured_self_type(self.db()) {
            if stype.is_trait_instance(self.db())
                && stype.is_ref_or_mut(self.db())
            {
                self.state
                    .diagnostics
                    .default_method_capturing_self(self.file(), node.location);
            }
        }

        node.closure_id = Some(closure);
        node.resolved_type
    }

    /// Processes a regular reference to a constant (i.e. `FOO`).
    ///
    /// If a constant has a source/receiver (e.g. `stdio.STDOUT`), it's
    /// processed as a method call, and not by this method, hence we ignore the
    /// `source` field of the HIR node.
    fn constant(
        &mut self,
        node: &mut hir::ConstantRef,
        scope: &mut LexicalScope,
        receiver: bool,
    ) -> TypeRef {
        let module = self.module;
        let (rec, rec_id, rec_kind, method) = {
            let rec = scope.surrounding_type;
            let rec_id = rec.as_type_enum(self.db()).unwrap();

            match rec_id.lookup_method(self.db(), &node.name, module, false) {
                MethodLookup::Ok(method) => {
                    let rec_info =
                        Receiver::without_receiver(self.db(), method);

                    (rec, rec_id, rec_info, method)
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(&node.name, rec, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(&node.name, rec, node.location);

                    return TypeRef::Error;
                }
                _ => match module.use_symbol(self.db_mut(), &node.name) {
                    Some(Symbol::Constant(id)) => {
                        node.resolved_type = id.value_type(self.db());
                        node.kind = ConstantKind::Constant(id);

                        return node.resolved_type;
                    }
                    Some(Symbol::Type(id)) if receiver => {
                        return TypeRef::Owned(TypeEnum::Type(id));
                    }
                    Some(Symbol::Type(_) | Symbol::Trait(_)) if !receiver => {
                        self.state.diagnostics.symbol_not_a_value(
                            &node.name,
                            self.file(),
                            node.location,
                        );

                        return TypeRef::Error;
                    }
                    Some(Symbol::Method(method)) => {
                        let id = method.module(self.db());

                        (
                            TypeRef::module(id),
                            TypeEnum::Module(id),
                            Receiver::with_module(self.db(), method),
                            method,
                        )
                    }
                    _ => {
                        self.state.diagnostics.undefined_symbol(
                            &node.name,
                            self.file(),
                            node.location,
                        );

                        return TypeRef::Error;
                    }
                },
            }
        };

        let loc = node.location;
        let mut call = MethodCall::new(
            self.state,
            module,
            Some((self.method, &self.self_types)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, node.usage, loc);

        let returns = call.return_type;

        node.kind = ConstantKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
            type_arguments: call.type_arguments,
        });

        if node.usage.is_unused() && returns.must_use(self.db(), rec) {
            self.state.diagnostics.unused_result(self.file(), node.location);
        }

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
            self.lookup_variable(name, scope, node.location)
        {
            node.kind = IdentifierKind::Variable(var);

            return typ;
        }

        let (rec, rec_id, rec_kind, method) = {
            let rec = scope.surrounding_type;
            let rec_id = rec.as_type_enum(self.db()).unwrap();

            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(method) if method.is_extern(self.db()) => {
                    (rec, rec_id, Receiver::Extern, method)
                }
                MethodLookup::Ok(method) => {
                    self.check_if_self_is_allowed(scope, node.location);

                    if method.is_instance(self.db()) {
                        scope.mark_closures_as_capturing_self(self.db_mut());
                    }

                    let rec_info =
                        Receiver::without_receiver(self.db(), method);

                    (rec, rec_id, rec_info, method)
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(name, rec, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(name, rec, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::Private => {
                    self.private_method_call(name, node.location);

                    return TypeRef::Error;
                }
                _ => {
                    if let Some(Symbol::Module(id)) =
                        module.use_symbol(self.db_mut(), name)
                    {
                        if !receiver {
                            self.state.diagnostics.symbol_not_a_value(
                                name,
                                self.file(),
                                node.location,
                            );

                            return TypeRef::Error;
                        }

                        return TypeRef::module(id);
                    }

                    if let Some(Symbol::Method(method)) =
                        module.use_symbol(self.db_mut(), name)
                    {
                        let id = method.module(self.db());

                        (
                            TypeRef::module(id),
                            TypeEnum::Module(id),
                            Receiver::with_module(self.db(), method),
                            method,
                        )
                    } else {
                        self.state.diagnostics.undefined_symbol(
                            name,
                            self.file(),
                            node.location,
                        );

                        return TypeRef::Error;
                    }
                }
            }
        };

        let loc = node.location;
        let mut call = MethodCall::new(
            self.state,
            module,
            Some((self.method, &self.self_types)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, node.usage, loc);

        let returns = call.return_type;

        node.kind = IdentifierKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
            type_arguments: call.type_arguments,
        });

        if node.usage.is_unused() && returns.must_use(self.db(), rec) {
            self.state.diagnostics.unused_result(self.file(), node.location);
        }

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
                node.location,
            );

            return TypeRef::Error;
        };

        let (mut ret, as_pointer) = self.borrow_field(
            scope.surrounding_type,
            raw_type,
            node.in_mut,
            false,
        );

        if scope.in_recover() {
            ret = ret.as_uni_borrow(self.db());
        }

        node.info = Some(FieldInfo {
            type_id: scope.surrounding_type.type_id(self.db()).unwrap(),
            id: field,
            variable_type: ret,
            as_pointer,
        });

        scope.mark_closures_as_capturing_self(self.db_mut());
        ret
    }

    fn assign_field(
        &mut self,
        node: &mut hir::AssignField,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((field, typ)) = self.check_field_assignment(
            &node.field.name,
            &mut node.value,
            node.field.location,
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
            node.field.location,
            scope,
        ) {
            if scope.in_recover() && !typ.is_value_type(self.db()) {
                self.state.diagnostics.unsendable_old_value(
                    &node.field.name,
                    self.file(),
                    node.location,
                );
            }

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
        location: Location,
        scope: &mut LexicalScope,
    ) -> Option<(FieldId, TypeRef)> {
        let val_type = self.expression(value_node, scope);

        let (field, var_type) = if let Some(typ) = self.field_type(name) {
            typ
        } else {
            self.state.diagnostics.undefined_field(name, self.file(), location);

            return None;
        };

        if !field.is_mutable(self.db()) {
            self.state.diagnostics.immutable_field_assignment(
                name,
                self.file(),
                location,
            );

            return None;
        }

        if !TypeChecker::check(self.db(), val_type, var_type) {
            self.state.diagnostics.type_error(
                format_type(self.db(), val_type),
                format_type(self.db(), var_type),
                self.file(),
                location,
            );
        }

        if !scope.surrounding_type.allow_field_assignments(self.db()) {
            self.state.diagnostics.invalid_field_assignment(
                &format_type(self.db(), scope.surrounding_type),
                self.file(),
                location,
            );
        }

        if scope.in_recover() && !val_type.is_sendable(self.db()) {
            self.state.diagnostics.unsendable_field_value(
                name,
                self.fmt(val_type),
                self.file(),
                location,
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
                "the 'break' keyword can only be used inside loops",
                self.file(),
                node.location,
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
                "the 'next' keyword can only be used inside loops",
                self.file(),
                node.location,
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

        if !TypeChecker::check_return(
            self.db(),
            returned,
            expected,
            self.self_type,
        ) {
            self.state.diagnostics.type_error(
                format_type(self.db(), returned),
                format_type(self.db(), expected),
                self.file(),
                node.location,
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
        let throw_type = if scope.in_recover() && expr.is_owned(self.db()) {
            expr.as_uni(self.db())
        } else {
            expr
        };

        node.return_type = ret_type;
        node.resolved_type = throw_type;

        match ret_type.throw_kind(self.db()) {
            ThrowKind::Unknown | ThrowKind::Option(_) => self
                .state
                .diagnostics
                .throw_not_available(self.file(), node.location),
            ThrowKind::Infer(pid) => {
                let var = TypeRef::placeholder(self.db_mut(), None);
                let typ = TypeRef::result_type(self.db_mut(), var, expr);

                pid.assign(self.db_mut(), typ);
            }
            ThrowKind::Result(ret_ok, ret_err) => {
                if !TypeChecker::check_return(
                    self.db(),
                    throw_type,
                    ret_err,
                    self.self_type,
                ) {
                    self.state.diagnostics.invalid_throw(
                        ThrowKind::Result(ret_ok, expr)
                            .throw_type_name(self.db(), ret_ok),
                        format_type(self.db(), ret_type),
                        self.file(),
                        node.location,
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
        let mut types = Vec::new();
        let mut has_nil = false;

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

            let typ = self.last_expression_type(&mut case.body, &mut new_scope);

            // If a `case` returns `Nil`, we ignore the return values of all
            // cases. If a case returns `Never`, we only ignore that `case` when
            // type checking.
            if typ.is_nil(self.db()) {
                has_nil = true;
            } else if !typ.is_never(self.db()) {
                let loc =
                    case.body.last().map_or(case.location, |n| n.location());

                types.push((typ, loc));
            }
        }

        if has_nil || types.is_empty() {
            node.write_result = false;
            node.resolved_type = TypeRef::nil();
        } else {
            let first = types[0].0;

            node.resolved_type = first;

            for (typ, loc) in types.drain(1..) {
                if !TypeChecker::check_return(
                    self.db(),
                    typ,
                    first,
                    self.self_type,
                ) {
                    self.state.diagnostics.type_error(
                        format_type(self.db(), typ),
                        format_type(self.db(), first),
                        self.file(),
                        loc,
                    );
                }
            }
        }

        node.resolved_type
    }

    fn ref_expression(
        &mut self,
        node: &mut hir::Ref,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr = self.expression(&mut node.value, scope);

        if !expr.allow_as_ref(self.db()) {
            self.state.diagnostics.invalid_borrow(
                self.fmt(expr),
                self.file(),
                node.location,
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
        if let hir::Expression::IdentifierRef(n) = &mut node.value {
            if let Some(m) = self.module.method(self.db(), &n.name) {
                if m.uses_c_calling_convention(self.db()) {
                    node.pointer_to_method = Some(m);
                    node.resolved_type = TypeRef::pointer(TypeEnum::Foreign(
                        types::ForeignType::Int(8, Sign::Unsigned),
                    ));

                    return node.resolved_type;
                }
            }
        }

        let expr = self.expression(&mut node.value, scope);

        if !expr.allow_as_mut(self.db()) {
            self.state.diagnostics.invalid_borrow(
                self.fmt(expr),
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        node.resolved_type = if expr.is_value_type(self.db()) {
            if expr.is_foreign_type(self.db()) {
                expr.as_pointer(self.db())
            } else {
                expr
            }
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
        } else if last_type.is_uni_value(db) {
            last_type.as_owned(db)
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "values of type '{}' can't be recovered",
                    self.fmt(last_type)
                ),
                self.file(),
                node.location,
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
        let rec_id = if let Some(id) = self.receiver_id(receiver, node.location)
        {
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
                self.private_method_call(&setter, node.location);

                return TypeRef::Error;
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(&setter, receiver, node.location);

                return TypeRef::Error;
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(&setter, receiver, node.location);

                return TypeRef::Error;
            }
            MethodLookup::None => {
                if self.assign_field_with_receiver(
                    node, receiver, rec_id, value, scope,
                ) {
                    return TypeRef::nil();
                }

                return match receiver {
                    TypeRef::Pointer(id)
                        if node.name.name == DEREF_POINTER_FIELD =>
                    {
                        let exp = id.as_type_for_pointer();

                        if !TypeChecker::check(self.db(), value, exp) {
                            self.state.diagnostics.type_error(
                                self.fmt(value),
                                self.fmt(exp),
                                self.file(),
                                node.location,
                            );
                        }

                        node.kind = CallKind::WritePointer;
                        TypeRef::nil()
                    }
                    _ => {
                        self.state.diagnostics.undefined_method(
                            &setter,
                            self.fmt(receiver),
                            self.file(),
                            node.location,
                        );

                        TypeRef::Error
                    }
                };
            }
        };

        let loc = node.name.location;
        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.method, &self.self_types)),
            receiver,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        node.expected_type =
            self.positional_argument(&mut call, 0, &mut node.value, scope);

        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, node.usage, loc);

        let returns = call.return_type;
        let rec_info = Receiver::with_receiver(self.db(), receiver, method);

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
            type_arguments: call.type_arguments,
        });

        if node.usage.is_unused() && returns.must_use(self.db(), receiver) {
            self.state.diagnostics.unused_result(self.file(), node.location);
        }

        returns
    }

    fn replace_setter(
        &mut self,
        node: &mut hir::ReplaceSetter,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let rec = self.expression(&mut node.receiver, scope);
        let Some(rec_id) = self.receiver_id(rec, node.location) else {
            return TypeRef::Error;
        };
        let Some((ins, field)) =
            self.lookup_field_with_receiver(rec_id, &node.name)
        else {
            self.state.diagnostics.undefined_field(
                &node.name.name,
                self.file(),
                node.name.location,
            );

            return TypeRef::Error;
        };

        if !field.is_mutable(self.db()) {
            self.state.diagnostics.immutable_field_assignment(
                &node.name.name,
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        if !rec.allow_field_assignments(self.db()) {
            self.state.diagnostics.invalid_field_assignment(
                &format_type(self.db(), rec),
                self.module.file(self.db()),
                node.location,
            );

            return TypeRef::Error;
        }

        let value = self.expression(&mut node.value, scope);
        let targs = TypeArguments::for_type(self.db(), ins);
        let raw_type = field.value_type(self.db());
        let bounds = self.bounds;
        let var_type =
            TypeResolver::new(self.db_mut(), &targs, bounds).resolve(raw_type);
        let value = value.cast_according_to(self.db(), var_type);

        if !TypeChecker::check(self.db(), value, var_type) {
            self.state.diagnostics.type_error(
                self.fmt(value),
                self.fmt(var_type),
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        if rec.require_sendable_arguments(self.db())
            && !value.is_sendable(self.db())
        {
            self.state.diagnostics.unsendable_field_value(
                &node.name.name,
                self.fmt(value),
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        if scope.in_recover() && !var_type.is_value_type(self.db()) {
            self.state.diagnostics.unsendable_old_value(
                &node.name.name,
                self.file(),
                node.location,
            );
        }

        node.field_id = Some(field);
        node.resolved_type = var_type;
        var_type
    }

    fn assign_field_with_receiver(
        &mut self,
        node: &mut hir::AssignSetter,
        receiver: TypeRef,
        receiver_id: TypeEnum,
        value: TypeRef,
        scope: &mut LexicalScope,
    ) -> bool {
        let name = &node.name.name;
        let Some((ins, field)) =
            self.lookup_field_with_receiver(receiver_id, &node.name)
        else {
            return false;
        };

        // When using `self.field = value`, none of the below is applicable, nor
        // do we need to calculate the field type as it's already cached.
        if receiver_id == self.self_type {
            return if let Some((field, typ)) = self.check_field_assignment(
                name,
                &mut node.value,
                node.name.location,
                scope,
            ) {
                node.kind = CallKind::SetField(FieldInfo {
                    type_id: receiver.type_id(self.db()).unwrap(),
                    id: field,
                    variable_type: typ,
                    as_pointer: false,
                });

                true
            } else {
                false
            };
        }

        if !field.is_mutable(self.db()) {
            self.state.diagnostics.immutable_field_assignment(
                name,
                self.file(),
                node.location,
            );

            return true;
        }

        if !receiver.allow_field_assignments(self.db()) {
            self.state.diagnostics.invalid_field_assignment(
                &format_type(self.db(), receiver),
                self.module.file(self.db()),
                node.location,
            );
        }

        let targs = TypeArguments::for_type(self.db(), ins);
        let raw_type = field.value_type(self.db());
        let bounds = self.bounds;
        let var_type =
            TypeResolver::new(self.db_mut(), &targs, bounds).resolve(raw_type);

        let value = value.cast_according_to(self.db(), var_type);

        if !TypeChecker::check(self.db(), value, var_type) {
            self.state.diagnostics.type_error(
                self.fmt(value),
                self.fmt(var_type),
                self.file(),
                node.location,
            );
        }

        if receiver.require_sendable_arguments(self.db())
            && !value.is_sendable(self.db())
        {
            self.state.diagnostics.unsendable_field_value(
                name,
                self.fmt(value),
                self.file(),
                node.location,
            );
        }

        node.kind = CallKind::SetField(FieldInfo {
            type_id: ins.instance_of(),
            id: field,
            variable_type: var_type,
            as_pointer: false,
        });

        true
    }

    fn call(
        &mut self,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
        as_receiver: bool,
    ) -> TypeRef {
        let (rec, typ) = if let Some((rec, allow_type_private)) =
            node.receiver.as_mut().map(|r| self.call_receiver(r, scope))
        {
            if let Some(closure) = rec.closure_id(self.db()) {
                (rec, self.call_closure(rec, closure, node, scope))
            } else {
                let res = self.call_with_receiver(
                    rec,
                    node,
                    scope,
                    allow_type_private,
                    as_receiver,
                );

                (rec, res)
            }
        } else {
            (scope.surrounding_type, self.call_without_receiver(node, scope))
        };

        if node.usage.is_unused() && typ.must_use(self.db(), rec) {
            self.state.diagnostics.unused_result(self.file(), node.location);
        }

        typ
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
                node.location,
            );

            return TypeRef::Error;
        }

        if !receiver.allow_mutating(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                "closures can only be called using owned or mutable references",
                self.file(),
                node.location,
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
                node.location,
            );

            return TypeRef::Error;
        }

        let targs = TypeArguments::new();

        for (index, arg_node) in node.arguments.iter_mut().enumerate() {
            let exp = closure
                .positional_argument_input_type(self.db(), index)
                .unwrap()
                .as_rigid_type(&mut self.state.db, self.bounds);

            let pos_node = match arg_node {
                hir::Argument::Positional(expr) => expr,
                hir::Argument::Named(n) => {
                    self.state
                        .diagnostics
                        .closure_with_named_argument(self.file(), n.location);

                    continue;
                }
            };

            let rec = receiver.as_type_enum(self.db()).unwrap();
            let given = self
                .argument_expression(
                    &mut pos_node.value,
                    rec,
                    exp,
                    &targs,
                    scope,
                )
                .cast_according_to(self.db(), exp);

            if !TypeChecker::check(self.db(), given, exp) {
                self.state.diagnostics.type_error(
                    format_type(self.db(), given),
                    format_type(self.db(), exp),
                    self.file(),
                    pos_node.value.location(),
                );
            }

            pos_node.expected_type = exp;
        }

        let returns = {
            let raw = closure.return_type(self.db());

            TypeResolver::new(&mut self.state.db, &targs, self.bounds)
                .resolve(raw)
        };

        node.kind =
            CallKind::CallClosure(ClosureCallInfo { id: closure, returns });

        returns
    }

    fn call_with_receiver(
        &mut self,
        receiver: TypeRef,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
        allow_type_private: bool,
        as_receiver: bool,
    ) -> TypeRef {
        let rec_id = if let Some(id) = self.receiver_id(receiver, node.location)
        {
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
                self.private_method_call(&node.name.name, node.location);

                return TypeRef::Error;
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(
                    &node.name.name,
                    receiver,
                    node.location,
                );

                return TypeRef::Error;
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(
                    &node.name.name,
                    receiver,
                    node.location,
                );

                return TypeRef::Error;
            }
            MethodLookup::None if node.arguments.is_empty() && !node.parens => {
                if let Some(typ) =
                    self.field_with_receiver(node, receiver, rec_id)
                {
                    return typ;
                }

                if let TypeEnum::Module(id) = rec_id {
                    match id.use_symbol(self.db_mut(), &node.name.name) {
                        Some(Symbol::Constant(id)) => {
                            node.kind = CallKind::GetConstant(id);

                            return id.value_type(self.db());
                        }
                        Some(Symbol::Type(id)) if as_receiver => {
                            return TypeRef::Owned(TypeEnum::Type(id));
                        }
                        Some(Symbol::Type(_) | Symbol::Trait(_))
                            if !as_receiver =>
                        {
                            self.state.diagnostics.symbol_not_a_value(
                                &node.name.name,
                                self.file(),
                                node.location,
                            );

                            return TypeRef::Error;
                        }
                        _ => {}
                    }
                }

                return match receiver {
                    TypeRef::Pointer(id)
                        if node.name.name == DEREF_POINTER_FIELD =>
                    {
                        let ret = id.as_type_for_pointer();

                        node.kind = CallKind::ReadPointer(ret);
                        ret
                    }
                    _ => {
                        self.state.diagnostics.undefined_method(
                            &node.name.name,
                            self.fmt(receiver),
                            self.file(),
                            node.location,
                        );

                        TypeRef::Error
                    }
                };
            }
            MethodLookup::None => {
                if let TypeEnum::Module(mod_id) = rec_id {
                    if let Some(Symbol::Type(id)) =
                        mod_id.use_symbol(self.db_mut(), &node.name.name)
                    {
                        return self.new_type_instance(node, scope, id);
                    }
                }

                self.state.diagnostics.undefined_method(
                    &node.name.name,
                    self.fmt(receiver),
                    self.file(),
                    node.location,
                );

                return TypeRef::Error;
            }
        };

        let loc = node.name.location;
        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.method, &self.self_types)),
            receiver,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, node.usage, loc);

        let returns = call.return_type;
        let rec_info = Receiver::with_receiver(self.db(), receiver, method);

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
            type_arguments: call.type_arguments,
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
        let rec_id = rec.as_type_enum(self.db()).unwrap();
        let (rec_info, rec, rec_id, method) =
            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(method) => {
                    self.check_if_self_is_allowed(scope, node.location);

                    if method.is_instance(self.db()) {
                        scope.mark_closures_as_capturing_self(self.db_mut());
                    }

                    let rec_info =
                        Receiver::without_receiver(self.db(), method);

                    (rec_info, rec, rec_id, method)
                }
                MethodLookup::Private => {
                    self.private_method_call(name, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(name, rec, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(name, rec, node.location);

                    return TypeRef::Error;
                }
                MethodLookup::None => {
                    if name == SELF_TYPE {
                        match self.self_type {
                            TypeEnum::TypeInstance(ins) => {
                                let id = ins.instance_of();

                                return self.new_type_instance(node, scope, id);
                            }
                            TypeEnum::Type(id) => {
                                return self.new_type_instance(node, scope, id);
                            }
                            _ => {}
                        }
                    }

                    match self.module.use_symbol(self.db_mut(), name) {
                        Some(Symbol::Method(method)) => {
                            // The receiver of imported module methods is the
                            // module they are defined in.
                            //
                            // Private module methods can't be imported, so we
                            // don't need to check the visibility here.
                            let mod_id = method.module(self.db());
                            let id = TypeEnum::Module(mod_id);
                            let mod_typ = TypeRef::Owned(id);

                            (
                                Receiver::with_module(self.db(), method),
                                mod_typ,
                                id,
                                method,
                            )
                        }
                        Some(Symbol::Type(id)) => {
                            return self.new_type_instance(node, scope, id);
                        }
                        _ => {
                            self.state.diagnostics.undefined_symbol(
                                name,
                                self.file(),
                                node.name.location,
                            );

                            return TypeRef::Error;
                        }
                    }
                }
            };

        let loc = node.name.location;
        let mut call = MethodCall::new(
            self.state,
            self.module,
            Some((self.method, &self.self_types)),
            rec,
            rec_id,
            method,
        );

        call.check_mutability(self.state, loc);
        call.check_type_bounds(self.state, loc);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_arguments(self.state, loc);
        call.resolve_return_type(self.state);
        call.check_sendable(self.state, node.usage, loc);

        let returns = call.return_type;

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: rec_info,
            returns,
            dynamic: rec_id.use_dynamic_dispatch(),
            type_arguments: call.type_arguments,
        });

        returns
    }

    fn new_type_instance(
        &mut self,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
        type_id: TypeId,
    ) -> TypeRef {
        if type_id.is_builtin() && !self.module.is_std(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                "instances of builtin types can't be created using the \
                type literal syntax",
                self.file(),
                node.location,
            );
        }

        let kind = type_id.kind(self.db());
        let require_send = kind.is_async();
        let ins = TypeInstance::empty(self.db_mut(), type_id);
        let mut assigned = HashSet::new();
        let mut fields = Vec::new();

        for (idx, arg) in node.arguments.iter_mut().enumerate() {
            let (field, val_expr) = match arg {
                hir::Argument::Positional(n) => {
                    let field = if let Some(v) =
                        type_id.field_by_index(self.db(), idx)
                    {
                        v
                    } else {
                        let num = type_id.number_of_fields(self.db());

                        self.state.diagnostics.error(
                            DiagnosticId::InvalidSymbol,
                            format!(
                                "the field index {} is out of bounds \
                                    (total number of fields: {})",
                                idx, num,
                            ),
                            self.file(),
                            n.value.location(),
                        );

                        continue;
                    };

                    (field, &mut n.value)
                }
                hir::Argument::Named(n) => {
                    let field = if let Some(v) =
                        type_id.field(self.db(), &n.name.name)
                    {
                        v
                    } else {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidSymbol,
                            format!(
                                "the field '{}' is undefined",
                                &n.name.name
                            ),
                            self.file(),
                            n.location,
                        );

                        continue;
                    };

                    (field, &mut n.value)
                }
            };

            let name = field.name(self.db()).clone();

            if !field.is_visible_to(self.db(), self.module) {
                self.state.diagnostics.private_field(
                    &name,
                    self.file(),
                    node.location,
                );
            }

            let targs = ins.type_arguments(self.db()).unwrap().clone();

            // The field type is the _raw_ type, but we want one that takes into
            // account what we have inferred thus far. Consider the following
            // code:
            //
            //     type Foo[T] {
            //       let @a: Option[Option[T]]
            //     }
            //
            //     Foo(a: Option.None) as Foo[Int]
            //
            // When comparing the `Option.None` against `Option[Option[T]]`, we
            // want to make sure that the `T` is later (as part of the cast)
            // inferred as `Int`. If we use the raw type as-is this won't
            // happen, because the inner `Option[T]` won't use a type
            // placeholder as the value assigned to `T`, instead using `T`
            // itself.
            //
            // Failing to handle this correctly will break type specialization,
            // as we'd end up trying to specialize the `T` in the inner
            // `Option[T]` without a meaningful type being assigned to it.
            let expected = {
                let raw = field.value_type(self.db());
                let bounds = TypeBounds::new();

                TypeResolver::new(self.db_mut(), &targs, &bounds).resolve(raw)
            };

            let value = self.expression(val_expr, scope);
            let value_casted = value.cast_according_to(self.db(), expected);
            let checker = TypeChecker::new(self.db());
            let mut env =
                Environment::new(value_casted.type_arguments(self.db()), targs);

            if !checker.run(value_casted, expected, &mut env) {
                self.state.diagnostics.type_error(
                    format_type_with_arguments(self.db(), &env.left, value),
                    format_type_with_arguments(self.db(), &env.right, expected),
                    self.file(),
                    val_expr.location(),
                );
            }

            // The values assigned to fields of processes must be sendable as
            // part of the assignment. If the value is a `recover` expression
            // that returns an owned value we _do_ allow this, because at that
            // point the owned value is sendable.
            if require_send
                && !value.is_sendable(self.db())
                && !val_expr.is_recover()
            {
                self.state.diagnostics.unsendable_field_value(
                    &name,
                    format_type(self.db(), value),
                    self.file(),
                    val_expr.location(),
                );
            }

            if assigned.contains(&name) {
                self.state.diagnostics.error(
                    DiagnosticId::DuplicateSymbol,
                    format!("the field '{}' is already assigned", name),
                    self.file(),
                    arg.location(),
                );
            }

            assigned.insert(name);
            fields.push((field, expected));
        }

        // For extern types we allow either all fields to be specified, or all
        // fields to be left out. The latter is useful when dealing with C
        // structures that start on the stack as uninitialized data and are
        // initialized using a dedicated function.
        //
        // If an extern type has one or more fields specifid, then we require
        // _all_ fields to be specified, as leaving out fields in this case is
        // likely the result of a mistake.
        if !kind.is_extern() || !assigned.is_empty() {
            for field in type_id.field_names(self.db()) {
                if assigned.contains(&field) {
                    continue;
                }

                self.state.diagnostics.error(
                    DiagnosticId::MissingField,
                    format!("the field '{}' must be assigned a value", field),
                    self.file(),
                    node.location,
                );
            }
        }

        let resolved_type = TypeRef::Owned(TypeEnum::TypeInstance(ins));

        node.kind = CallKind::TypeInstance(types::TypeInstanceInfo {
            type_id,
            resolved_type,
            fields,
        });

        resolved_type
    }

    fn field_with_receiver(
        &mut self,
        node: &mut hir::Call,
        receiver: TypeRef,
        receiver_id: TypeEnum,
    ) -> Option<TypeRef> {
        let (ins, field) =
            self.lookup_field_with_receiver(receiver_id, &node.name)?;
        let raw_type = field.value_type(self.db_mut());
        let immutable = receiver.is_ref(self.db_mut());
        let args = ins.type_arguments(self.db_mut()).unwrap().clone();
        let bounds = self.bounds;
        let returns = TypeResolver::new(self.db_mut(), &args, bounds)
            .with_immutable(immutable)
            .resolve(raw_type);

        let (mut returns, as_pointer) =
            self.borrow_field(receiver, returns, node.in_mut, true);

        if receiver.require_sendable_arguments(self.db()) {
            returns = returns.as_uni_borrow(self.db());
        }

        node.kind = CallKind::GetField(FieldInfo {
            id: field,
            type_id: ins.instance_of(),
            variable_type: returns,
            as_pointer,
        });

        Some(returns)
    }

    fn builtin_call(
        &mut self,
        node: &mut hir::BuiltinCall,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let args: Vec<_> = node
            .arguments
            .iter_mut()
            .map(|n| self.expression(n, scope))
            .collect();

        let id = if let Some(id) = self.db().intrinsic(&node.name.name) {
            id
        } else {
            self.state.diagnostics.undefined_symbol(
                &node.name.name,
                self.file(),
                node.name.location,
            );

            return TypeRef::Error;
        };

        let returns = id.return_type(self.db(), &args);

        node.info = Some(IntrinsicCall { id, returns });
        returns
    }

    fn type_cast(
        &mut self,
        node: &mut hir::TypeCast,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr_type = self.expression(&mut node.value, scope);
        let rules =
            Rules { type_parameters_as_rigid: true, ..Default::default() };

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

        if !TypeChecker::check_cast(self.db_mut(), expr_type, cast_type) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCast,
                format!(
                    "the type '{}' can't be cast to '{}'",
                    format_type(self.db(), expr_type),
                    format_type(self.db(), cast_type)
                ),
                self.file(),
                node.location,
            );

            return TypeRef::Error;
        }

        node.resolved_type = cast_type;
        node.resolved_type
    }

    fn size_of(&mut self, node: &mut hir::SizeOf) -> TypeRef {
        node.resolved_type =
            self.type_signature(&mut node.argument, self.self_type);

        TypeRef::int()
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
            (ThrowKind::Option(some), ThrowKind::Infer(pid)) => {
                let inferred = TypeRef::option_type(self.db_mut(), some);

                pid.assign(self.db_mut(), inferred);
                return some;
            }
            (ThrowKind::Result(ok, err), ThrowKind::Infer(pid)) => {
                let inferred = TypeRef::result_type(self.db_mut(), ok, err);

                pid.assign(self.db_mut(), inferred);
                return ok;
            }
            (
                ThrowKind::Result(ok, expr_err),
                ThrowKind::Result(ret_ok, ret_err),
            ) => {
                if TypeChecker::check_return(
                    self.db(),
                    expr_err,
                    ret_err,
                    self.self_type,
                ) {
                    return ok;
                }

                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ret_ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location,
                );
            }
            (ThrowKind::Unknown | ThrowKind::Infer(_), _) => {
                self.state.diagnostics.invalid_try(
                    format_type(self.db(), expr),
                    self.file(),
                    node.expression.location(),
                );
            }
            (_, ThrowKind::Unknown) => {
                self.state
                    .diagnostics
                    .try_not_available(self.file(), node.location);
            }
            (ThrowKind::Option(_), ThrowKind::Result(ret_ok, _)) => {
                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ret_ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location,
                );
            }
            (ThrowKind::Result(_, _), ThrowKind::Option(ok)) => {
                self.state.diagnostics.invalid_throw(
                    expr_kind.throw_type_name(self.db(), ok),
                    format_type(self.db(), ret_type),
                    self.file(),
                    node.location,
                );
            }
        }

        TypeRef::Error
    }

    fn receiver_id(
        &mut self,
        receiver: TypeRef,
        location: Location,
    ) -> Option<TypeEnum> {
        match receiver.as_type_enum(self.db()) {
            Ok(id) => Some(id),
            Err(TypeRef::Error) => None,
            Err(TypeRef::Placeholder(_)) => {
                self.state.diagnostics.cant_infer_type(
                    format_type(self.db(), receiver),
                    self.file(),
                    location,
                );

                None
            }
            Err(typ) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidCall,
                    format!(
                        "methods can't be called on values of type '{}'",
                        self.fmt(typ)
                    ),
                    self.file(),
                    location,
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
                self.module.use_symbol(self.db_mut(), &src.name)
            {
                Ok(module.use_symbol(self.db_mut(), name))
            } else {
                self.state.diagnostics.symbol_not_a_module(
                    &src.name,
                    self.file(),
                    src.location,
                );

                Err(())
            }
        } else {
            Ok(self.module.use_symbol(self.db_mut(), name))
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
            hir::Expression::Call(ref mut n) => self.call(n, scope, true),
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
                    n.expected_type = self.positional_argument(
                        call,
                        index,
                        &mut n.value,
                        scope,
                    );
                }
                hir::Argument::Named(ref mut n) => {
                    n.expected_type = self.named_argument(call, n, scope);
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
    ) -> TypeRef {
        call.arguments += 1;

        if let Some(expected) =
            call.method.positional_argument_input_type(self.db(), index)
        {
            let given = self.argument_expression(
                node,
                call.receiver.as_type_enum(self.db()).unwrap(),
                expected,
                &call.type_arguments,
                scope,
            );

            call.check_argument(self.state, given, expected, node.location())
        } else {
            self.expression(node, scope)
        }
    }

    fn named_argument(
        &mut self,
        call: &mut MethodCall,
        node: &mut hir::NamedArgument,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let name = &node.name.name;

        if let Some((index, expected)) =
            call.method.named_argument(self.db(), name)
        {
            // We persist the index so we don't need to look it up again when
            // lowering to MIR.
            node.index = index;

            let given = self.argument_expression(
                &mut node.value,
                call.receiver.as_type_enum(self.db()).unwrap(),
                expected,
                &call.type_arguments,
                scope,
            );

            if call.named_arguments.contains(name) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidCall,
                    format!(
                        "the named argument '{}' is already specified",
                        name
                    ),
                    self.file(),
                    node.name.location,
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
            )
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "the argument '{}' isn't defined by the method '{}'",
                    name,
                    call.method.name(self.db()),
                ),
                self.file(),
                node.name.location,
            );

            TypeRef::Error
        }
    }

    fn check_if_self_is_allowed(
        &mut self,
        scope: &LexicalScope,
        location: Location,
    ) {
        if scope.surrounding_type.is_value_type(self.db()) {
            return;
        }

        if scope.in_closure_in_recover() {
            self.state
                .diagnostics
                .self_in_closure_in_recover(self.file(), location);
        }
    }

    fn require_boolean(&mut self, typ: TypeRef, location: Location) {
        if typ == TypeRef::Error || typ.is_bool(self.db()) {
            return;
        }

        self.state.diagnostics.error(
            DiagnosticId::InvalidType,
            format!(
                "expected a 'Bool', 'ref Bool' or 'mut Bool', \
                found '{}' instead",
                format_type(self.db(), typ),
            ),
            self.file(),
            location,
        );
    }

    fn type_signature(
        &mut self,
        node: &mut hir::Type,
        self_type: TypeEnum,
    ) -> TypeRef {
        // Within the bodies of static and module methods, the meaning of `Self`
        // is either unclear or there simply is no type to replace it with.
        let allow_self = matches!(
            self_type,
            TypeEnum::TypeInstance(_) | TypeEnum::TraitInstance(_)
        );
        let rules = Rules {
            type_parameters_as_rigid: true,
            allow_self,
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
        location: Location,
    ) {
        self.state.diagnostics.invalid_static_call(
            name,
            self.fmt(receiver),
            self.file(),
            location,
        );
    }

    fn invalid_instance_call(
        &mut self,
        name: &str,
        receiver: TypeRef,
        location: Location,
    ) {
        self.state.diagnostics.invalid_instance_call(
            name,
            self.fmt(receiver),
            self.file(),
            location,
        );
    }

    fn private_method_call(&mut self, name: &str, location: Location) {
        self.state.diagnostics.private_method_call(name, self.file(), location);
    }

    fn lookup_variable(
        &mut self,
        name: &str,
        scope: &LexicalScope,
        location: Location,
    ) -> Option<(VariableId, TypeRef, bool)> {
        let mut source = Some(scope);
        let mut scopes = Vec::new();
        let mut var = None;

        while let Some(current) = source {
            scopes.push(current);

            if let Some(variable) = current.variables.variable(name) {
                var = Some(variable);
                break;
            }

            source = current.parent;
        }

        let var = var?;
        let mut capture_as = var.value_type(self.db());
        let mut expose_as = capture_as;
        let mut captured = false;
        let mut allow_assignment = true;

        // The scope the variable is defined in doesn't influence its type, so
        // we ignore it.
        scopes.pop();

        // We now process the remaining sub scopes outside-in, which is needed
        // so we can determine the correct type of the variable, and whether
        // capturing is allowed or not.
        while let Some(scope) = scopes.pop() {
            match scope.kind {
                ScopeKind::Recover => {
                    expose_as = expose_as.as_uni_borrow(self.db());
                }
                ScopeKind::Closure(closure) => {
                    // Closures are always captured by value because of the
                    // following:
                    //
                    // 1. Capturing them by borrowing will almost always result
                    //    in the captured closure being dropped prematurely,
                    //    unless one explicitly uses `fn move`.
                    // 2. Closure borrows can't be persisted.
                    let moving = closure.is_moving(self.db())
                        || capture_as.is_closure(self.db());

                    if !expose_as.allow_capturing(self.db(), moving) {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidSymbol,
                            format!(
                                "the variable '{}' exists, but its type \
                                ('{}') prevents it from being captured",
                                name,
                                self.fmt(expose_as)
                            ),
                            self.file(),
                            location,
                        );
                    }

                    // The outer-most closure may capture the value as an owned
                    // value, if the closure is a moving closure. For nested
                    // closures the capture type is always a reference.
                    if captured {
                        capture_as = expose_as;
                    } else if moving && capture_as.is_uni_value(self.db()) {
                        // When an `fn move` captures a `uni T`, we capture it
                        // as-is but expose it as `mut T`, making it easier to
                        // work with the value. This is safe because:
                        //
                        // 1. The closure itself doesn't care about the
                        //    uniqueness constraint
                        // 2. We can't move the value out of the closure and
                        //    back into a `uni T` value
                        //
                        // We don't change the capture type such that `fn move`
                        // closures capturing `uni T` values can still be
                        // inferred as `uni fn move` closures.
                        expose_as =
                            capture_as.as_owned(self.db()).as_mut(self.db());
                    } else {
                        if !moving {
                            capture_as = capture_as.as_mut(self.db());
                        }

                        expose_as = expose_as.as_mut(self.db());
                    }

                    closure.add_capture(self.db_mut(), var, capture_as);
                    captured = true;

                    // Captured variables can only be assigned by moving
                    // closures, as non-moving closures store references to the
                    // captured values, not the values themselves. We can't
                    // assign such captures a new value, as the value referred
                    // to (in most cases at least) wouldn't outlive the closure.
                    allow_assignment = moving;
                }
                _ => {}
            }
        }

        Some((var, expose_as, allow_assignment))
    }

    fn lookup_field_with_receiver(
        &mut self,
        receiver_id: TypeEnum,
        name: &hir::Identifier,
    ) -> Option<(TypeInstance, FieldId)> {
        let (ins, field) = if let TypeEnum::TypeInstance(ins) = receiver_id {
            ins.instance_of()
                .field(self.db(), &name.name)
                .map(|field| (ins, field))
        } else {
            None
        }?;

        // We disallow `receiver.field` even when `receiver` is `self`, because
        // we can't tell the difference between two different instances of the
        // same non-generic process (e.g. every instance `type async Foo {}`
        // has the same TypeId).
        if ins.instance_of().kind(self.db()).is_async() {
            self.state.diagnostics.unavailable_process_field(
                &name.name,
                self.file(),
                name.location,
            );
        }

        if !field.is_visible_to(self.db(), self.module) {
            self.state.diagnostics.private_field(
                &name.name,
                self.file(),
                name.location,
            );
        }

        Some((ins, field))
    }

    fn borrow_field(
        &self,
        receiver: TypeRef,
        typ: TypeRef,
        in_mut: bool,
        borrow: bool,
    ) -> (TypeRef, bool) {
        let db = self.db();

        // Foreign types are as raw pointers when necessary for FFI purposes.
        if (in_mut && typ.is_foreign_type(db)) || typ.is_extern_instance(db) {
            return (typ.as_pointer(db), true);
        }

        let res = if typ.is_value_type(db) {
            typ
        } else if receiver.is_ref(db) {
            typ.as_ref(db)
        } else if receiver.is_mut(db) || borrow {
            typ.as_mut(db)
        } else {
            typ
        };

        (res, false)
    }
}
