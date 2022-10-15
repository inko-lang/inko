//! Passes for type-checking method body and constant expressions.
use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use crate::type_check::{DefineAndCheckTypeSignature, Rules, TypeScope};
use ast::source_location::SourceLocation;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use types::{
    format_type, format_type_with_context, format_type_with_self, Block,
    BuiltinCallInfo, BuiltinFunctionKind, CallInfo, CallKind, ClassId,
    ClassInstance, Closure, ClosureCallInfo, ClosureId, CompilerMacro,
    ConstantKind, ConstantPatternKind, Database, FieldId, FieldInfo,
    IdentifierKind, MethodId, MethodLookup, MethodSource, ModuleId, Receiver,
    Symbol, TraitId, TraitInstance, TypeArguments, TypeBounds, TypeContext,
    TypeId, TypeRef, Variable, VariableId, CALL_METHOD, STRING_MODULE,
    TO_STRING_TRAIT,
};

const IGNORE_VARIABLE: &str = "_";

const INDEX_METHOD: &str = "index";
const INDEX_MUT_METHOD: &str = "index_mut";
const INDEX_MODULE: &str = "std::index";
const INDEX_TRAIT: &str = "Index";
const INDEX_MUT_TRAIT: &str = "IndexMut";

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
pub struct VariableScope {
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

    /// The throw type of the surrounding block.
    throw_type: TypeRef,

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
    fn method(
        self_type: TypeRef,
        return_type: TypeRef,
        throw_type: TypeRef,
    ) -> Self {
        Self {
            kind: ScopeKind::Method,
            variables: VariableScope::new(),
            surrounding_type: self_type,
            return_type,
            throw_type,
            parent: None,
            in_closure: false,
            break_in_loop: Cell::new(false),
        }
    }

    fn inherit(&'a self, kind: ScopeKind) -> Self {
        Self {
            kind,
            surrounding_type: self.surrounding_type,
            throw_type: self.throw_type,
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

    /// A context for type-checking and substituting types.
    context: TypeContext,

    /// The type of the method's receiver.
    receiver: TypeRef,

    /// The number of arguments specified.
    arguments: usize,

    /// The named arguments that have been specified thus far.
    named_arguments: HashSet<String>,

    /// If input/output types should be limited to sendable types.
    require_sendable: bool,
}

impl MethodCall {
    fn new(
        state: &State,
        module: ModuleId,
        receiver: TypeRef,
        receiver_id: TypeId,
        method: MethodId,
    ) -> Self {
        let mut args = TypeArguments::new();

        // The method call needs access to the type arguments of the receiver.
        // So given a `pop -> T` method for `Array[Int]`, we want to be able to
        // map `T` to `Int`. Since a TypeContext only has a single
        // `TypeArguments` structure (to simplify lookups), we copy the
        // arguments of the receiver into this temporary collection of
        // arguments.
        match receiver_id {
            TypeId::ClassInstance(ins) => {
                ins.copy_type_arguments_into(&state.db, &mut args);
            }
            TypeId::TraitInstance(ins) => {
                ins.copy_type_arguments_into(&state.db, &mut args);
            }
            _ => {}
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
        match method.source(&state.db) {
            MethodSource::BoundedImplementation(ins)
            | MethodSource::Implementation(ins) => {
                ins.copy_type_arguments_into(&state.db, &mut args);
            }
            _ => {}
        }

        let require_sendable = receiver.require_sendable_arguments(&state.db)
            && !method.is_moving(&state.db);

        Self {
            module,
            method,
            receiver,
            context: TypeContext::with_arguments(receiver_id, args),
            arguments: 0,
            named_arguments: HashSet::new(),
            require_sendable,
        }
    }

    fn check_bounded_implementation(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) {
        if let MethodSource::BoundedImplementation(trait_ins) =
            self.method.source(&state.db)
        {
            if self.receiver_id().implements_trait_instance(
                &mut state.db,
                trait_ins,
                &mut self.context,
            ) {
                return;
            }

            let method_name = self.method.name(&state.db).clone();
            let rec_name = format_type_with_context(
                &state.db,
                &self.context,
                self.receiver_id(),
            );
            let trait_name =
                format_type_with_context(&state.db, &self.context, trait_ins);

            state.diagnostics.error(
                DiagnosticId::InvalidSymbol,
                format!(
                    "The method '{}' exists for type '{}', \
                    but requires this type to implement trait '{}'",
                    method_name, rec_name, trait_name
                ),
                self.module.file(&state.db),
                location.clone(),
            );
        }
    }

    fn check_argument_count(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) {
        let expected = self.method.number_of_arguments(&state.db);

        if self.arguments == expected {
            return;
        }

        state.diagnostics.incorrect_call_arguments(
            self.arguments,
            expected,
            self.module.file(&state.db),
            location.clone(),
        );
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
                    format_type_with_context(&state.db, &self.context, rec)
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
                    format_type_with_context(&state.db, &self.context, rec)
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

        if self.require_sendable && !given.is_sendable(&state.db) {
            state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The receiver ('{}') of this call requires sendable \
                    arguments, but '{}' isn't sendable",
                    format_type_with_context(
                        &state.db,
                        &self.context,
                        self.receiver
                    ),
                    format_type_with_context(&state.db, &self.context, given),
                ),
                self.module.file(&state.db),
                location.clone(),
            );
        }

        if given.type_check(&mut state.db, expected, &mut self.context, true) {
            return;
        }

        state.diagnostics.type_error(
            format_type_with_context(&state.db, &self.context, given),
            format_type_with_context(&state.db, &self.context, expected),
            self.module.file(&state.db),
            location.clone(),
        );
    }

    fn update_receiver_type_arguments(
        &mut self,
        state: &mut State,
        surrounding_type: TypeRef,
    ) {
        // We don't update the type arguments of `self`, as that messes up
        // future method calls acting on the same type.
        match surrounding_type {
            TypeRef::Owned(id) | TypeRef::Ref(id)
                if id == self.receiver_id() =>
            {
                return;
            }
            TypeRef::OwnedSelf | TypeRef::RefSelf => return,
            _ => {}
        }

        let db = &mut state.db;
        let args = &self.context.type_arguments;

        // As part of the method call we use a temporary collection of type
        // arguments. Once the call is done, newly assigned type parameters that
        // belong to the receiver need to be copied into the receiver's type
        // arguments. This ensures that future method calls observe the newly
        // assigned type parameters.
        match self.receiver_id() {
            TypeId::ClassInstance(ins) => ins.copy_new_arguments_from(db, args),
            TypeId::TraitInstance(ins) => ins.copy_new_arguments_from(db, args),
            _ => {}
        }
    }

    fn throw_type(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) -> TypeRef {
        let typ = self.method.throw_type(&state.db).inferred(
            &mut state.db,
            &mut self.context,
            false,
        );

        if !self.output_type_is_sendable(state, typ) {
            let name = self.method.name(&state.db);

            state.diagnostics.unsendable_throw_type(
                name,
                format_type_with_context(&state.db, &self.context, typ),
                self.module.file(&state.db),
                location.clone(),
            );
        }

        typ
    }

    fn return_type(
        &mut self,
        state: &mut State,
        location: &SourceLocation,
    ) -> TypeRef {
        let typ = self.method.return_type(&state.db).inferred(
            &mut state.db,
            &mut self.context,
            false,
        );

        if !self.output_type_is_sendable(state, typ) {
            let name = self.method.name(&state.db);

            state.diagnostics.unsendable_return_type(
                name,
                format_type_with_context(&state.db, &self.context, typ),
                self.module.file(&state.db),
                location.clone(),
            );
        }

        typ
    }

    fn receiver_id(&self) -> TypeId {
        self.context.self_type
    }

    fn output_type_is_sendable(&self, state: &State, typ: TypeRef) -> bool {
        if !self.require_sendable {
            return true;
        }

        // If a method is immutable and doesn't define any arguments, any
        // returned or thrown value that is sendable must have been created as
        // part of the call, and thus can't have any outside references pointing
        // to it. This allows such methods to return/throw owned values, while
        // still allowing the use of such methods on unique receivers.
        //
        // Note that this check still enforces sendable sub values, meaning it's
        // invalid to return e.g. `Array[ref Thing]`, as `ref Thing` isn't
        // sendable.
        if self.method.number_of_arguments(&state.db) == 0
            && self.method.is_immutable(&state.db)
        {
            typ.is_sendable_output(&state.db)
        } else {
            typ.is_sendable(&state.db)
        }
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
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineConstants { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expression in module.expressions.iter_mut() {
            if let hir::TopLevelExpression::Constant(ref mut n) = expression {
                self.define_constant(n);
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
        let bounds = TypeBounds::new();
        let id = node.class_id.unwrap();
        let num_methods = id.number_of_methods(self.db());

        if num_methods > METHODS_IN_CLASS_LIMIT {
            self.state.diagnostics.error(
                DiagnosticId::InvalidClass,
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
                    self.define_instance_method(n, &bounds);
                }
                hir::ClassExpression::StaticMethod(ref mut n) => {
                    self.define_static_method(n);
                }
                _ => {}
            }
        }
    }

    fn reopen_class(&mut self, node: &mut hir::ReopenClass) {
        let bounds = TypeBounds::new();

        for node in &mut node.body {
            match node {
                hir::ReopenClassExpression::InstanceMethod(ref mut n) => {
                    self.define_instance_method(n, &bounds)
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
        let bounds = TypeBounds::new();

        self.verify_type_parameter_requirements(&node.type_parameters);
        self.verify_required_traits(
            &node.requirements,
            node.trait_id.unwrap().required_traits(self.db()),
        );

        for node in &mut node.body {
            if let hir::TraitExpression::InstanceMethod(ref mut n) = node {
                self.define_instance_method(n, &bounds);
            }
        }
    }

    fn implement_trait(&mut self, node: &mut hir::ImplementTrait) {
        let class_id = node.class_instance.unwrap().instance_of();
        let trait_id = node.trait_instance.unwrap().instance_of();
        let bounds = class_id
            .trait_implementation(self.db(), trait_id)
            .map(|i| i.bounds.clone())
            .unwrap();

        for n in &mut node.body {
            self.define_instance_method(n, &bounds);
        }
    }

    fn define_module_method(&mut self, node: &mut hir::DefineModuleMethod) {
        let method = node.method_id.unwrap();
        let stype = method.self_type(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let throws =
            method.throw_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns, throws);

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

        checker.check_if_throws(throws, &node.location);
    }

    fn define_instance_method(
        &mut self,
        node: &mut hir::DefineInstanceMethod,
        bounds: &TypeBounds,
    ) {
        let method = node.method_id.unwrap();
        let stype = method.self_type(self.db());
        let receiver = method.receiver(self.db());
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), bounds);
        let throws =
            method.throw_type(self.db()).as_rigid_type(self.db_mut(), bounds);
        let mut scope = LexicalScope::method(receiver, returns, throws);

        self.verify_type_parameter_requirements(&node.type_parameters);

        for arg in method.arguments(self.db()) {
            scope.variables.add_variable(arg.name, arg.variable);
        }

        self.define_field_types(receiver, method, bounds);

        let mut checker = CheckMethodBody::new(
            self.state,
            self.module,
            method,
            stype,
            bounds,
        );

        checker.expressions_with_return(
            returns,
            &mut node.body,
            &mut scope,
            &node.location,
        );

        checker.check_if_throws(throws, &node.location);
    }

    fn define_async_method(&mut self, node: &mut hir::DefineAsyncMethod) {
        let method = node.method_id.unwrap();
        let stype = method.self_type(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let throws =
            method.throw_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns, throws);

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

        checker.check_if_throws(throws, &node.location);
    }

    fn define_static_method(&mut self, node: &mut hir::DefineStaticMethod) {
        let method = node.method_id.unwrap();
        let stype = method.self_type(self.db());
        let receiver = method.receiver(self.db());
        let bounds = TypeBounds::new();
        let returns =
            method.return_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let throws =
            method.throw_type(self.db()).as_rigid_type(self.db_mut(), &bounds);
        let mut scope = LexicalScope::method(receiver, returns, throws);

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

        checker.check_if_throws(throws, &node.location);
    }

    fn define_field_types(
        &mut self,
        receiver: TypeRef,
        method: MethodId,
        bounds: &TypeBounds,
    ) {
        for field in receiver.fields(self.db()) {
            let name = field.name(self.db()).clone();
            let typ = field
                .value_type(self.db())
                .as_rigid_type(self.db_mut(), bounds);

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
    module_type: TypeRef,
}

impl<'a> CheckConstant<'a> {
    fn new(state: &'a mut State, module: ModuleId) -> Self {
        let module_type = TypeRef::Owned(TypeId::Module(module));

        Self { state, module, module_type }
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

        let mod_type = self.module_type;
        let mut call =
            MethodCall::new(self.state, self.module, left, left_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        self.positional_argument(&mut call, &mut node.right);
        call.check_argument_count(self.state, &node.location);
        call.update_receiver_type_arguments(self.state, mod_type);

        node.resolved_type = call.return_type(self.state, &node.location);
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
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    node.location.clone(),
                );

                TypeRef::Error
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
            let stype = TypeId::Module(self.module);
            let &first = types.first().unwrap();
            let mut ctx = TypeContext::new(stype);

            for (&typ, node) in types[1..].iter().zip(node.values[1..].iter()) {
                if !typ.type_check(self.db_mut(), first, &mut ctx, true) {
                    self.state.diagnostics.type_error(
                        format_type_with_context(self.db(), &ctx, typ),
                        format_type_with_context(self.db(), &ctx, first),
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
            ClassInstance::generic_with_types(
                self.db_mut(),
                ClassId::array(),
                types,
            ),
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
        let stype = TypeId::Module(self.module);
        let rec_id = match receiver.type_id(self.db(), stype) {
            Ok(id) => id,
            Err(TypeRef::Error) => return None,
            Err(typ) => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type_with_self(self.db(), stype, typ),
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
                    format_type_with_self(self.db(), stype, receiver),
                    self.file(),
                    location.clone(),
                );
            }
            MethodLookup::StaticOnInstance => {
                self.state.diagnostics.invalid_static_call(
                    name,
                    format_type_with_self(self.db(), stype, receiver),
                    self.file(),
                    location.clone(),
                );
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    name,
                    format_type_with_self(self.db(), stype, receiver),
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

    /// The type for `Self`, excluding ownership.
    self_type: TypeId,

    /// Any bounds to apply to type parameters.
    bounds: &'a TypeBounds,

    /// If a value is thrown from this body.
    thrown: bool,
}

impl<'a> CheckMethodBody<'a> {
    fn new(
        state: &'a mut State,
        module: ModuleId,
        method: MethodId,
        self_type: TypeId,
        bounds: &'a TypeBounds,
    ) -> Self {
        Self { state, module, method, self_type, bounds, thrown: false }
    }

    fn check_if_throws(
        &mut self,
        expected: TypeRef,
        location: &SourceLocation,
    ) {
        if !expected.is_present(self.db()) || self.thrown {
            return;
        }

        self.state.diagnostics.missing_throw(
            self.fmt(expected),
            self.file(),
            location.clone(),
        );
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
        let mut ctx = TypeContext::new(self.self_type);

        if returns.is_nil(self.db(), self.self_type) {
            // When the return type is `Nil` (explicit or not), we just ignore
            // whatever value is returned.
            return;
        }

        if !typ.type_check(self.db_mut(), returns, &mut ctx, true) {
            let loc =
                nodes.last().map(|n| n.location()).unwrap_or(fallback_location);

            self.state.diagnostics.type_error(
                format_type_with_context(self.db(), &ctx, typ),
                format_type_with_context(self.db(), &ctx, returns),
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
            hir::Expression::AsyncCall(ref mut n) => self.async_call(n, scope),
            hir::Expression::Break(ref n) => self.break_expression(n, scope),
            hir::Expression::BuiltinCall(ref mut n) => {
                self.builtin_call(n, scope)
            }
            hir::Expression::Call(ref mut n) => self.call(n, scope),
            hir::Expression::Closure(ref mut n) => self.closure(n, None, scope),
            hir::Expression::ConstantRef(ref mut n) => self.constant(n, scope),
            hir::Expression::DefineVariable(ref mut n) => {
                self.define_variable(n, scope)
            }
            hir::Expression::False(ref mut n) => self.false_literal(n),
            hir::Expression::FieldRef(ref mut n) => self.field(n, scope),
            hir::Expression::Float(ref mut n) => self.float_literal(n, scope),
            hir::Expression::IdentifierRef(ref mut n) => {
                self.identifier(n, scope)
            }
            hir::Expression::ClassLiteral(ref mut n) => {
                self.class_literal(n, scope)
            }
            hir::Expression::Int(ref mut n) => self.int_literal(n, scope),
            hir::Expression::Invalid(_) => TypeRef::Error,
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
            hir::Expression::Index(ref mut n) => {
                self.index_expression(n, scope)
            }
        }
    }

    fn input_expression(
        &mut self,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let typ = self.expression(node, scope);

        if typ.is_value_type(self.db()) {
            typ.as_owned(self.db())
        } else {
            typ
        }
    }

    fn argument_expression(
        &mut self,
        expected_type: TypeRef,
        node: &mut hir::Expression,
        scope: &mut LexicalScope,
        context: &mut TypeContext,
    ) -> TypeRef {
        match node {
            hir::Expression::Closure(ref mut n) => {
                let expected = expected_type
                    .closure_id(self.db(), self.self_type)
                    .map(|f| (f, expected_type, context));

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
                    let stype = self.self_type;

                    if val != TypeRef::Error
                        && !val.is_string(self.db(), self.self_type)
                    {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidType,
                            format!(
                                "Expected a 'String', 'ref String' or \
                                'mut String', found '{}' instead",
                                format_type_with_self(self.db(), stype, val)
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
            let mut ctx = TypeContext::new(self.self_type);

            for (&typ, node) in types[1..].iter().zip(node.values[1..].iter()) {
                if !typ.type_check(self.db_mut(), first, &mut ctx, true) {
                    self.state.diagnostics.type_error(
                        format_type_with_context(self.db(), &ctx, typ),
                        format_type_with_context(self.db(), &ctx, first),
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

        let ins = ClassInstance::generic_with_types(
            self.db_mut(),
            ClassId::array(),
            types,
        );
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
            ClassInstance::generic_with_types(
                self.db_mut(),
                class,
                types.clone(),
            ),
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
        let ins = if class.is_generic(self.db()) {
            ClassInstance::generic_with_placeholders(self.db_mut(), class)
        } else {
            ClassInstance::new(class)
        };

        let mut ctx =
            TypeContext::for_class_instance(self.db(), self.self_type, ins);
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
                    DiagnosticId::PrivateSymbol,
                    format!("The field '{}' is private", name),
                    self.file(),
                    node.location.clone(),
                );
            }

            let expected = field_id.value_type(self.db());
            let value = self.expression(&mut field.value, scope);
            let value_casted = value.cast_according_to(expected, self.db());

            if !value_casted.type_check(self.db_mut(), expected, &mut ctx, true)
            {
                self.state.diagnostics.type_error(
                    format_type_with_context(self.db(), &ctx, value),
                    format_type_with_context(self.db(), &ctx, expected),
                    self.file(),
                    field.value.location().clone(),
                );
            }

            if require_send && !value.is_sendable(self.db()) {
                self.state.diagnostics.unsendable_field_value(
                    name,
                    format_type_with_context(self.db(), &ctx, value),
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

        ins.copy_new_arguments_from(self.db_mut(), &ctx.type_arguments);

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
                &format_type_with_self(self.db(), self.self_type, value_type),
                self.file(),
                node.value.location().clone(),
            );
        }

        let var_type = if let Some(tnode) = node.value_type.as_mut() {
            let exp_type = self.type_signature(tnode, self.self_type);
            let mut typ_ctx = TypeContext::new(self.self_type);
            let value_casted =
                value_type.cast_according_to(exp_type, self.db());

            if !value_casted.type_check(
                self.db_mut(),
                exp_type,
                &mut typ_ctx,
                true,
            ) {
                let stype = self.self_type;

                self.state.diagnostics.type_error(
                    format_type_with_self(self.db(), stype, value_type),
                    format_type_with_self(self.db(), stype, exp_type),
                    self.file(),
                    node.location.clone(),
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
            let mut typ_ctx = TypeContext::new(self.self_type);

            if !value_type.type_check(
                self.db_mut(),
                exp_type,
                &mut typ_ctx,
                true,
            ) {
                let stype = self.self_type;

                self.state.diagnostics.pattern_type_error(
                    format_type_with_self(self.db(), stype, value_type),
                    format_type_with_self(self.db(), stype, exp_type),
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
            let mut ctx = TypeContext::new(self.self_type);
            let ex_type = existing.value_type(self.db());

            if !var_type.type_check(self.db_mut(), ex_type, &mut ctx, true) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "The type of this variable is defined as '{}' \
                        in another pattern, but here its type is '{}'",
                        format_type_with_context(self.db(), &ctx, ex_type),
                        format_type_with_context(self.db(), &ctx, var_type),
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

        if let Some(ins) =
            value_type.as_enum_instance(self.db(), self.self_type)
        {
            let variant =
                if let Some(v) = ins.instance_of().variant(self.db(), name) {
                    v
                } else {
                    self.state.diagnostics.undefined_variant(
                        name,
                        format_type_with_self(
                            self.db(),
                            self.self_type,
                            value_type,
                        ),
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
                let cid = typ.class_id(self.db(), self.self_type).unwrap();

                node.kind = if cid == ClassId::int() {
                    ConstantPatternKind::Int(id)
                } else if cid == ClassId::string() {
                    ConstantPatternKind::String(id)
                } else {
                    self.state.diagnostics.error(
                        DiagnosticId::InvalidPattern,
                        format!(
                            "Expected a 'String' or 'Int', found '{}' instead",
                            format_type_with_self(
                                self.db(),
                                self.self_type,
                                typ
                            ),
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

        let mut typ_ctx = TypeContext::new(self.self_type);

        if !value_type.type_check(self.db_mut(), exp_type, &mut typ_ctx, true) {
            let self_type = self.self_type;

            self.state.diagnostics.pattern_type_error(
                format_type_with_self(self.db(), self_type, value_type),
                format_type_with_self(self.db(), self_type, exp_type),
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
                        format_type_with_self(
                            self.db(),
                            self.self_type,
                            value_type
                        ),
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
                if ins.instance_of().kind(self.db()).is_regular() =>
            {
                ins
            }
            _ => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "This pattern expects a regular class instance, \
                        but the input type is '{}'",
                        format_type_with_self(
                            self.db(),
                            self.self_type,
                            value_type
                        ),
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
                    format_type_with_self(
                        self.db(),
                        self.self_type,
                        value_type
                    )
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
        let mut ctx =
            TypeContext::for_class_instance(self.db(), self.self_type, ins);

        for node in &mut node.values {
            let name = &node.field.name;
            let field = if let Some(f) = class.field(self.db(), name) {
                f
            } else {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "The type '{}' doesn't define the field '{}'",
                        format_type_with_self(
                            self.db(),
                            self.self_type,
                            value_type
                        ),
                        name
                    ),
                    self.file(),
                    node.field.location.clone(),
                );

                self.pattern(&mut node.pattern, TypeRef::Error, pattern);
                continue;
            };

            let field_type = field
                .value_type(self.db())
                .inferred(self.db_mut(), &mut ctx, immutable)
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
        let mut typ_ctx = TypeContext::new(self.self_type);
        let compare = if input_type.is_owned_or_uni(self.db()) {
            input_type
        } else {
            // This ensures we can compare e.g. a `ref Int` to an integer
            // pattern.
            input_type.as_owned(self.db())
        };

        if !compare.type_check(self.db_mut(), pattern_type, &mut typ_ctx, true)
        {
            let self_type = self.self_type;

            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type of this pattern is '{}', \
                    but the input type is '{}'",
                    format_type_with_self(self.db(), self_type, pattern_type),
                    format_type_with_self(self.db(), self_type, input_type),
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

        let ins = if let Some(ins) =
            value_type.as_enum_instance(self.db(), self.self_type)
        {
            ins
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "This pattern expects an enum class, \
                    but the input type is '{}'",
                    format_type_with_self(
                        self.db(),
                        self.self_type,
                        value_type
                    ),
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
                format_type_with_self(self.db(), self.self_type, value_type),
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
        let mut ctx = TypeContext::new(self.self_type);

        ins.copy_type_arguments_into(self.db(), &mut ctx.type_arguments);

        for (patt, member) in node.values.iter_mut().zip(members.into_iter()) {
            let typ = member
                .inferred(self.db_mut(), &mut ctx, immutable)
                .cast_according_to(value_type, self.db());

            self.pattern(patt, typ, pattern);
        }

        ins.copy_new_arguments_from(self.db_mut(), &ctx.type_arguments);

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
                &format_type_with_self(self.db(), self.self_type, val_type),
                self.file(),
                value_node.location().clone(),
            );
        }

        let var_type = var.value_type(self.db());
        let mut ctx = TypeContext::new(self.self_type);

        if !val_type.type_check(self.db_mut(), var_type, &mut ctx, true) {
            self.state.diagnostics.type_error(
                format_type_with_self(self.db(), self.self_type, val_type),
                format_type_with_self(self.db(), self.self_type, var_type),
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
        mut expected: Option<(ClosureId, TypeRef, &mut TypeContext)>,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let self_type = self.self_type;
        let moving = node.moving
            || expected
                .as_ref()
                .map_or(false, |(id, _, _)| id.is_moving(self.db()));

        let closure = Closure::alloc(self.db_mut(), moving);
        let throw_type = if let Some(n) = node.throw_type.as_mut() {
            self.type_signature(n, self_type)
        } else {
            let db = &mut self.state.db;

            expected
                .as_mut()
                .map(|(id, _, context)| {
                    id.throw_type(db).inferred(db, *context, false)
                })
                .unwrap_or_else(|| TypeRef::placeholder(self.db_mut()))
        };

        let return_type = if let Some(n) = node.return_type.as_mut() {
            self.type_signature(n, self_type)
        } else {
            let db = &mut self.state.db;

            expected
                .as_mut()
                .map(|(id, _, context)| {
                    id.return_type(db).inferred(db, *context, false)
                })
                .unwrap_or_else(|| TypeRef::placeholder(self.db_mut()))
        };

        closure.set_throw_type(self.db_mut(), throw_type);
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
            throw_type,
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
                let db = &mut self.state.db;

                expected
                    .as_mut()
                    .and_then(|(id, _, context)| {
                        id.positional_argument_input_type(db, index)
                            .map(|t| t.inferred(db, context, false))
                    })
                    .unwrap_or_else(|| TypeRef::placeholder(db))
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

        self.check_if_throws(throw_type, &node.location);

        if let TypeRef::Placeholder(id) = throw_type {
            if id.value(self.db()).is_none() {
                closure.set_throw_type(self.db_mut(), TypeRef::Never);
            }
        }

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

    /// A reference to a constant.
    ///
    /// Types are not allowed, as they can't be used as values (e.g. `return
    /// ToString` makes no sense).
    fn constant(
        &mut self,
        node: &mut hir::ConstantRef,
        scope: &LexicalScope,
    ) -> TypeRef {
        let name = &node.name;
        let module = self.module;
        let (rec, rec_id, rec_kind, method) = {
            let rec = scope.surrounding_type;
            let rec_id = rec.type_id(self.db(), self.self_type).unwrap();

            match rec_id.lookup_method(self.db(), name, module, false) {
                MethodLookup::Ok(method) => {
                    (rec, rec_id, Receiver::Implicit, method)
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
                        Ok(Some(Symbol::Class(_) | Symbol::Trait(_))) => {
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
                            Receiver::Module(id),
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

        let mut call = MethodCall::new(self.state, module, rec, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        call.check_argument_count(self.state, &node.location);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);

        self.check_missing_try(name, throws, &node.location);

        node.kind = ConstantKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            throws,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        node.resolved_type = returns;
        node.resolved_type
    }

    /// A constant used as the receiver of a method call.
    ///
    /// Unlike regular constant references, we do allow types here, as this is
    /// needed to support expressions such as `Array.new`.
    fn constant_receiver(&mut self, node: &mut hir::ConstantRef) -> TypeRef {
        let name = &node.name;
        let symbol = self.lookup_constant(name, node.source.as_ref());

        match symbol {
            Ok(Some(Symbol::Constant(id))) => {
                node.kind = ConstantKind::Constant(id);
                node.resolved_type = id.value_type(self.db());

                node.resolved_type
            }
            Ok(Some(Symbol::Class(id))) => {
                node.kind = ConstantKind::Class(id);
                node.resolved_type = TypeRef::Owned(TypeId::Class(id));

                node.resolved_type
            }
            Ok(_) => {
                self.state.diagnostics.undefined_symbol(
                    name,
                    self.file(),
                    node.location.clone(),
                );

                TypeRef::Error
            }
            Err(_) => TypeRef::Error,
        }
    }

    fn identifier(
        &mut self,
        node: &mut hir::IdentifierRef,
        scope: &mut LexicalScope,
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
            let stype = self.self_type;
            let rec = scope.surrounding_type;
            let rec_id = rec.type_id(self.db(), stype).unwrap();

            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(method) => {
                    self.check_if_self_is_allowed(scope, &node.location);
                    scope.mark_closures_as_capturing_self(self.db_mut());
                    (rec, rec_id, Receiver::Implicit, method)
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
                    if let Some((field, raw_type)) = self.field_type(name) {
                        let typ = self.field_reference(
                            raw_type,
                            scope,
                            &node.location,
                        );

                        node.kind = IdentifierKind::Field(FieldInfo {
                            id: field,
                            variable_type: typ,
                        });

                        return typ;
                    }

                    if let Some(Symbol::Module(id)) =
                        module.symbol(self.db(), name)
                    {
                        let typ = TypeRef::module(id);

                        node.kind = IdentifierKind::Module(id);

                        return typ;
                    }

                    if let Some(Symbol::Method(method)) =
                        module.symbol(self.db(), name)
                    {
                        let id = method.module(self.db());

                        (
                            TypeRef::module(id),
                            TypeId::Module(id),
                            Receiver::Module(id),
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

        let mut call = MethodCall::new(self.state, module, rec, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        call.check_argument_count(self.state, &node.location);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);

        self.check_missing_try(name, throws, &node.location);

        node.kind = IdentifierKind::Method(CallInfo {
            id: method,
            receiver: rec_kind,
            returns,
            throws,
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
                &format_type_with_self(self.db(), self.self_type, val_type),
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

        let stype = self.self_type;
        let mut ctx = TypeContext::new(stype);

        if !val_type.type_check(self.db_mut(), var_type, &mut ctx, true) {
            self.state.diagnostics.type_error(
                format_type_with_self(self.db(), stype, val_type),
                format_type_with_self(self.db(), stype, var_type),
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

        self.require_boolean(lhs, self.self_type, node.left.location());
        self.require_boolean(rhs, self.self_type, node.right.location());

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

        self.require_boolean(lhs, self.self_type, node.left.location());
        self.require_boolean(rhs, self.self_type, node.right.location());

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
        let mut ctx = TypeContext::new(self.self_type);

        if !returned.type_check(self.db_mut(), expected, &mut ctx, true) {
            self.state.diagnostics.type_error(
                format_type_with_context(self.db(), &ctx, returned),
                format_type_with_context(self.db(), &ctx, expected),
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
        let mut thrown = self.expression(&mut node.value, scope);
        let expected = scope.throw_type;
        let mut ctx = TypeContext::new(self.self_type);

        if scope.in_recover() && thrown.is_owned(self.db()) {
            thrown = thrown.as_uni(self.db());
        }

        if expected.is_never(self.db()) {
            self.state
                .diagnostics
                .throw_not_allowed(self.file(), node.location.clone());
        } else if !thrown.type_check(self.db_mut(), expected, &mut ctx, true) {
            self.state.diagnostics.type_error(
                format_type_with_context(self.db(), &ctx, thrown),
                format_type_with_context(self.db(), &ctx, expected),
                self.file(),
                node.location.clone(),
            );
        }

        node.resolved_type = thrown;
        self.thrown = true;

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

                self.require_boolean(typ, self.self_type, guard.location());
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

        if !expr.is_owned_or_uni(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidRef,
                format!(
                    "A 'ref T' can't be created from a value of type '{}'",
                    self.fmt(expr)
                ),
                self.file(),
                node.location.clone(),
            );
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

        if !expr.is_owned_or_uni(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidRef,
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
                DiagnosticId::InvalidRef,
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
        let loc = &node.location;
        let (receiver, allow_type_private) =
            self.call_receiver(&mut node.receiver, scope);
        let value = self.expression(&mut node.value, scope);
        let setter = node.name.name.clone() + "=";
        let module = self.module;
        let rec_id = if let Some(id) = self.receiver_id(receiver, loc) {
            id
        } else {
            return TypeRef::Error;
        };

        let method =
            match rec_id.lookup_method(
                self.db(),
                &setter,
                module,
                allow_type_private,
            ) {
                MethodLookup::Ok(id) => id,
                MethodLookup::Private => {
                    self.private_method_call(&setter, loc);

                    return TypeRef::Error;
                }
                MethodLookup::InstanceOnStatic => {
                    self.invalid_instance_call(&setter, receiver, loc);

                    return TypeRef::Error;
                }
                MethodLookup::StaticOnInstance => {
                    self.invalid_static_call(&setter, receiver, loc);

                    return TypeRef::Error;
                }
                MethodLookup::None => {
                    let field_name = &node.name.name;

                    if let TypeId::ClassInstance(ins) = rec_id {
                        if let Some(field) =
                            ins.instance_of().field(self.db(), field_name)
                        {
                            if !field.is_visible_to(self.db(), module) {
                                self.state.diagnostics.private_field(
                                    field_name,
                                    self.file(),
                                    loc.clone(),
                                );
                            }

                            if !receiver.allow_mutating() {
                                self.state.diagnostics.error(
                                    DiagnosticId::InvalidCall,
                                    format!(
                                    "Can't assign a new value to field '{}', \
                                    as its receiver is immutable",
                                    field_name,
                                ),
                                    self.module.file(self.db()),
                                    loc.clone(),
                                );
                            }

                            if node.else_block.is_some() {
                                self.state
                                    .diagnostics
                                    .never_throws(self.file(), loc.clone());
                            }

                            let mut ctx = TypeContext::for_class_instance(
                                self.db(),
                                self.self_type,
                                ins,
                            );
                            let var_type = field
                                .value_type(self.db())
                                .inferred(self.db_mut(), &mut ctx, false);
                            let value =
                                value.cast_according_to(var_type, self.db());

                            if !value.type_check(
                                self.db_mut(),
                                var_type,
                                &mut ctx,
                                true,
                            ) {
                                self.state.diagnostics.type_error(
                                    self.fmt(value),
                                    self.fmt(var_type),
                                    self.file(),
                                    loc.clone(),
                                );
                            }

                            if receiver.require_sendable_arguments(self.db())
                                && !value.is_sendable(self.db())
                            {
                                self.state.diagnostics.unsendable_field_value(
                                    field_name,
                                    self.fmt(value),
                                    self.file(),
                                    loc.clone(),
                                );
                            }

                            node.kind = CallKind::SetField(FieldInfo {
                                id: field,
                                variable_type: var_type,
                            });

                            return types::TypeRef::nil();
                        }
                    }

                    self.state.diagnostics.undefined_method(
                        &setter,
                        self.fmt(receiver),
                        self.file(),
                        loc.clone(),
                    );

                    return TypeRef::Error;
                }
            };

        let mut call =
            MethodCall::new(self.state, self.module, receiver, rec_id, method);

        call.check_mutability(self.state, loc);
        call.check_bounded_implementation(self.state, loc);
        self.positional_argument(&mut call, 0, &mut node.value, scope);
        call.check_argument_count(self.state, loc);
        call.update_receiver_type_arguments(self.state, scope.surrounding_type);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);

        if let Some(block) = node.else_block.as_mut() {
            if throws.is_never(self.db()) {
                self.state.diagnostics.never_throws(self.file(), loc.clone());
            }

            self.try_else_block(block, returns, throws, scope);
        } else {
            self.check_missing_try(&setter, throws, loc);
        }

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: Receiver::Explicit,
            returns,
            throws,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn call(
        &mut self,
        node: &mut hir::Call,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        if let Some((rec, allow_type_private)) =
            node.receiver.as_mut().map(|r| self.call_receiver(r, scope))
        {
            if let Some(closure) = rec.closure_id(self.db(), self.self_type) {
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

        let mut ctx = TypeContext::new(self.self_type);
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
                .argument_expression(exp, arg_expr_node, scope, &mut ctx)
                .cast_according_to(exp, self.db());

            if !given.type_check(self.db_mut(), exp, &mut ctx, true) {
                self.state.diagnostics.type_error(
                    format_type_with_context(self.db(), &ctx, given),
                    format_type_with_context(self.db(), &ctx, exp),
                    self.file(),
                    arg_expr_node.location().clone(),
                );
            }

            exp_args.push(exp);
        }

        let throws = closure
            .throw_type(self.db())
            .as_rigid_type(&mut self.state.db, self.bounds)
            .inferred(self.db_mut(), &mut ctx, false);

        let returns = closure
            .return_type(self.db())
            .as_rigid_type(&mut self.state.db, self.bounds)
            .inferred(self.db_mut(), &mut ctx, false);

        if let Some(block) = node.else_block.as_mut() {
            if throws.is_never(self.db()) {
                self.state
                    .diagnostics
                    .never_throws(self.file(), node.location.clone());
            }

            self.try_else_block(block, returns, throws, scope);
        } else {
            self.check_missing_try(CALL_METHOD, throws, &node.location);
        }

        node.kind = CallKind::ClosureCall(ClosureCallInfo {
            id: closure,
            expected_arguments: exp_args,
            returns,
            throws,
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
        let name = &node.name.name;
        let loc = &node.location;
        let module = self.module;
        let rec_id = if let Some(id) = self.receiver_id(receiver, loc) {
            id
        } else {
            return TypeRef::Error;
        };

        let method = match rec_id.lookup_method(
            self.db(),
            name,
            module,
            allow_type_private,
        ) {
            MethodLookup::Ok(id) => id,
            MethodLookup::Private => {
                self.private_method_call(name, loc);

                return TypeRef::Error;
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(name, receiver, loc);

                return TypeRef::Error;
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(name, receiver, loc);

                return TypeRef::Error;
            }
            MethodLookup::None if node.arguments.is_empty() => {
                if let TypeId::ClassInstance(ins) = rec_id {
                    if let Some(field) =
                        ins.instance_of().field(self.db(), name)
                    {
                        if !field.is_visible_to(self.db(), module) {
                            self.state.diagnostics.private_field(
                                &node.name.name,
                                self.file(),
                                node.location.clone(),
                            );
                        }

                        if node.else_block.is_some() {
                            self.state
                                .diagnostics
                                .never_throws(self.file(), loc.clone());
                        }

                        let mut ctx = TypeContext::new(self.self_type);
                        let db = self.db_mut();
                        let raw_typ = field.value_type(db);

                        ins.type_arguments(db)
                            .copy_into(&mut ctx.type_arguments);

                        let mut returns = if raw_typ.is_owned_or_uni(db) {
                            let typ = raw_typ.inferred(db, &mut ctx, false);

                            if receiver.is_ref(db) {
                                typ.as_ref(db)
                            } else {
                                typ.as_mut(db)
                            }
                        } else {
                            raw_typ.inferred(db, &mut ctx, receiver.is_ref(db))
                        };

                        returns = returns.value_type_as_owned(self.db());

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
                            variable_type: returns,
                        });

                        return returns;
                    }
                }

                self.state.diagnostics.undefined_method(
                    name,
                    self.fmt(receiver),
                    self.file(),
                    loc.clone(),
                );

                return TypeRef::Error;
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    name,
                    self.fmt(receiver),
                    self.file(),
                    loc.clone(),
                );

                return TypeRef::Error;
            }
        };

        let mut call =
            MethodCall::new(self.state, module, receiver, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_argument_count(self.state, &node.location);
        call.update_receiver_type_arguments(self.state, scope.surrounding_type);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);

        if let Some(block) = node.else_block.as_mut() {
            if throws.is_never(self.db()) {
                self.state
                    .diagnostics
                    .never_throws(self.file(), node.location.clone());
            }

            self.try_else_block(block, returns, throws, scope);
        } else {
            self.check_missing_try(name, throws, &node.location);
        }

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: Receiver::Explicit,
            returns,
            throws,
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
        let stype = self.self_type;
        let module = self.module;
        let rec = scope.surrounding_type;
        let rec_id = rec.type_id(self.db(), stype).unwrap();
        let (receiver_info, rec, rec_id, method) =
            match rec_id.lookup_method(self.db(), name, module, true) {
                MethodLookup::Ok(id) => {
                    self.check_if_self_is_allowed(scope, &node.location);
                    scope.mark_closures_as_capturing_self(self.db_mut());

                    (Receiver::Implicit, rec, rec_id, id)
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

                        (Receiver::Module(mod_id), mod_typ, id, method)
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

        let mut call =
            MethodCall::new(self.state, self.module, rec, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_argument_count(self.state, &node.location);
        call.update_receiver_type_arguments(self.state, scope.surrounding_type);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);

        if let Some(block) = node.else_block.as_mut() {
            if throws.is_never(self.db()) {
                self.state
                    .diagnostics
                    .never_throws(self.file(), node.location.clone());
            }

            self.try_else_block(block, returns, throws, scope);
        } else {
            self.check_missing_try(name, throws, &node.location);
        }

        node.kind = CallKind::Call(CallInfo {
            id: method,
            receiver: receiver_info,
            returns,
            throws,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn async_call(
        &mut self,
        node: &mut hir::AsyncCall,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let allow_type_private = node.receiver.is_self();
        let rec_type = self.expression(&mut node.receiver, scope);
        let name = &node.name.name;
        let (rec_id, method) = if let Some(found) = self.lookup_method(
            rec_type,
            name,
            &node.location,
            allow_type_private,
        ) {
            found
        } else {
            return TypeRef::Error;
        };

        if !matches!(
            rec_id,
            TypeId::ClassInstance(ins) if ins.instance_of().kind(self.db()).is_async()
        ) {
            let rec_name =
                format_type_with_self(self.db(), self.self_type, rec_type);

            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!("'{}' isn't an async type", rec_name),
                self.file(),
                node.receiver.location().clone(),
            );

            return TypeRef::Error;
        }

        let mut call =
            MethodCall::new(self.state, self.module, rec_type, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        self.call_arguments(&mut node.arguments, &mut call, scope);
        call.check_argument_count(self.state, &node.location);
        call.update_receiver_type_arguments(self.state, scope.surrounding_type);

        let returns = call.return_type(self.state, &node.location);
        let throws = call.throw_type(self.state, &node.location);
        let fut_class = ClassId::future();
        let mut fut_args = TypeArguments::new();
        let fut_params = fut_class.type_parameters(self.db());

        fut_args.assign(fut_params[0], returns);
        fut_args.assign(fut_params[1], throws);

        let throws = TypeRef::Never;
        let returns = TypeRef::Owned(TypeId::ClassInstance(
            ClassInstance::generic(self.db_mut(), fut_class, fut_args),
        ));

        node.info = Some(CallInfo {
            id: method,
            receiver: Receiver::Explicit,
            returns,
            throws,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn try_else_block(
        &mut self,
        node: &mut hir::ElseBlock,
        returns: TypeRef,
        throws: TypeRef,
        scope: &mut LexicalScope,
    ) {
        let mut new_scope = scope.inherit(ScopeKind::Regular);

        if let Some(var_def) = node.argument.as_mut() {
            let typ = if let Some(n) = var_def.value_type.as_mut() {
                let mut typ_ctx = TypeContext::new(self.self_type);
                let exp_type = self.type_signature(n, self.self_type);

                if !throws.type_check(
                    self.db_mut(),
                    exp_type,
                    &mut typ_ctx,
                    true,
                ) {
                    let self_type = self.self_type;

                    self.state.diagnostics.type_error(
                        format_type_with_self(self.db(), self_type, throws),
                        format_type_with_self(self.db(), self_type, exp_type),
                        self.file(),
                        node.location.clone(),
                    );
                }

                exp_type
            } else {
                throws
            };

            if var_def.name.name != IGNORE_VARIABLE {
                let var = new_scope.variables.new_variable(
                    self.db_mut(),
                    var_def.name.name.clone(),
                    typ,
                    false,
                );

                var_def.variable_id = Some(var);
            }
        }

        self.expressions_with_return(
            returns,
            &mut node.body,
            &mut new_scope,
            &node.location,
        );
    }

    fn builtin_call(
        &mut self,
        node: &mut hir::BuiltinCall,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let args: Vec<TypeRef> = node
            .arguments
            .iter_mut()
            .map(|n| self.expression(n, scope))
            .collect();

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

        // `try!` is desugared into `try x else (err) _INKO.panic_thrown(err)`.
        // This way we don't need to duplicate a lot of the `try` logic for
        // `try!`. We handle this case explicitly here to provide better error
        // message in case a thrown type doesn't implement ToString.
        if let BuiltinFunctionKind::Macro(CompilerMacro::PanicThrown) =
            id.kind(self.db())
        {
            let trait_id =
                self.db().trait_in_module(STRING_MODULE, TO_STRING_TRAIT);
            let arg = args[0];

            if !arg.implements_trait_id(self.db(), trait_id, self.self_type) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidType,
                    format!(
                        "This expression may panic with a value of type '{}', \
                        but this type doesn't implement '{}::{}'",
                        format_type_with_self(self.db(), self.self_type, arg),
                        STRING_MODULE,
                        TO_STRING_TRAIT
                    ),
                    self.file(),
                    node.location.clone(),
                );
            }
        }

        let returns = id.return_type(self.db());
        let throws = id.throw_type(self.db());

        if let Some(block) = node.else_block.as_mut() {
            self.try_else_block(block, returns, throws, scope);
        } else {
            self.check_missing_try(&node.name.name, throws, &node.location);
        }

        node.info = Some(BuiltinCallInfo { id, returns, throws });

        returns
    }

    fn type_cast(
        &mut self,
        node: &mut hir::TypeCast,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let expr_type = self.expression(&mut node.value, scope);
        let cast_type = self.type_signature(&mut node.cast_to, self.self_type);
        let mut ctx = TypeContext::new(self.self_type);

        if !expr_type.allow_cast_to(self.db_mut(), cast_type, &mut ctx) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type '{}' can't be cast to type '{}'",
                    format_type_with_context(self.db(), &ctx, expr_type),
                    format_type_with_context(self.db(), &ctx, cast_type)
                ),
                self.file(),
                node.location.clone(),
            );

            return TypeRef::Error;
        }

        node.resolved_type = cast_type;
        node.resolved_type
    }

    fn index_expression(
        &mut self,
        node: &mut hir::Index,
        scope: &mut LexicalScope,
    ) -> TypeRef {
        let index = self.db().trait_in_module(INDEX_MODULE, INDEX_TRAIT);
        let index_mut =
            self.db().trait_in_module(INDEX_MODULE, INDEX_MUT_TRAIT);

        let stype = self.self_type;
        let allow_type_private = node.receiver.is_self();
        let rec = self.expression(&mut node.receiver, scope);
        let name = if rec.allow_mutating()
            && rec.implements_trait_id(self.db(), index_mut, stype)
        {
            INDEX_MUT_METHOD
        } else if rec.implements_trait_id(self.db(), index, stype) {
            INDEX_METHOD
        } else {
            self.state.diagnostics.error(
                DiagnosticId::InvalidType,
                format!(
                    "The type '{typ}' must implement either \
                    {module}::{index} or {module}::{index_mut}",
                    typ = format_type_with_self(self.db(), stype, rec),
                    module = INDEX_MODULE,
                    index = INDEX_TRAIT,
                    index_mut = INDEX_MUT_TRAIT
                ),
                self.file(),
                node.receiver.location().clone(),
            );

            return TypeRef::Error;
        };

        let (rec_id, method) = if let Some(found) =
            self.lookup_method(rec, name, &node.location, allow_type_private)
        {
            found
        } else {
            return TypeRef::Error;
        };

        let mut call =
            MethodCall::new(self.state, self.module, rec, rec_id, method);

        call.check_mutability(self.state, &node.location);
        call.check_bounded_implementation(self.state, &node.location);
        self.positional_argument(&mut call, 0, &mut node.index, scope);
        call.check_argument_count(self.state, &node.location);
        call.update_receiver_type_arguments(self.state, scope.surrounding_type);

        let returns = call.return_type(self.state, &node.location);

        node.info = Some(CallInfo {
            id: method,
            receiver: Receiver::Explicit,
            returns,
            throws: TypeRef::Never,
            dynamic: rec_id.use_dynamic_dispatch(),
        });

        returns
    }

    fn lookup_method(
        &mut self,
        receiver: TypeRef,
        name: &str,
        location: &SourceLocation,
        allow_type_private: bool,
    ) -> Option<(TypeId, MethodId)> {
        let rec_id = self.receiver_id(receiver, location)?;

        match rec_id.lookup_method(
            self.db(),
            name,
            self.module,
            allow_type_private,
        ) {
            MethodLookup::Ok(id) => return Some((rec_id, id)),
            MethodLookup::Private => {
                self.private_method_call(name, location);
            }
            MethodLookup::InstanceOnStatic => {
                self.invalid_instance_call(name, receiver, location);
            }
            MethodLookup::StaticOnInstance => {
                self.invalid_static_call(name, receiver, location);
            }
            MethodLookup::None => {
                self.state.diagnostics.undefined_method(
                    name,
                    self.fmt(receiver),
                    self.file(),
                    location.clone(),
                );
            }
        }

        None
    }

    fn receiver_id(
        &mut self,
        receiver: TypeRef,
        location: &SourceLocation,
    ) -> Option<TypeId> {
        match receiver.type_id(self.db(), self.self_type) {
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
                self.constant_receiver(n)
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
                &mut call.context,
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
                &mut call.context,
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

    fn check_missing_try(
        &mut self,
        name: &str,
        throws: TypeRef,
        location: &SourceLocation,
    ) {
        if throws.is_present(self.db()) {
            self.state.diagnostics.error(
                DiagnosticId::InvalidCall,
                format!(
                    "The method '{}' may throw a value of type '{}', \
                    but the 'try' keyword is missing",
                    name,
                    format_type_with_self(self.db(), self.self_type, throws),
                ),
                self.file(),
                location.clone(),
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

    fn require_boolean(
        &mut self,
        typ: TypeRef,
        self_type: TypeId,
        location: &SourceLocation,
    ) {
        if typ == TypeRef::Error || typ.is_bool(self.db(), self.self_type) {
            return;
        }

        self.state.diagnostics.error(
            DiagnosticId::InvalidType,
            format!(
                "Expected a 'Bool', 'ref Bool' or 'mut Bool', \
                found '{}' instead",
                format_type_with_self(self.db(), self_type, typ),
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
        let typ = if scope.in_closure && raw_type.is_owned_or_uni(self.db()) {
            // Closures capture `self` as a whole, instead of capturing
            // individual fields. If `self` is owned, this means we have to
            // expose fields as references; not owned values.
            raw_type.as_mut(self.db())
        } else {
            raw_type.cast_according_to(scope.surrounding_type, self.db())
        };

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
        format_type_with_self(self.db(), self.self_type, typ)
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
