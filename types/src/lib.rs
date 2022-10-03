//! Structures for the various Inko types.
#![cfg_attr(feature = "cargo-clippy", allow(clippy::new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]

pub mod collections;
pub mod module_name;

use crate::collections::IndexMap;
use crate::module_name::ModuleName;
use bytecode::{BuiltinFunction as BIF, Opcode};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const INT_ID: u32 = 0;
pub const FLOAT_ID: u32 = 1;
pub const STRING_ID: u32 = 2;
const ARRAY_ID: u32 = 3;
pub const BOOLEAN_ID: u32 = 4;
const NIL_ID: u32 = 5;
const BYTE_ARRAY_ID: u32 = 6;
const FUTURE_ID: u32 = 7;

const TUPLE1_ID: u32 = 8;
const TUPLE2_ID: u32 = 9;
const TUPLE3_ID: u32 = 10;
const TUPLE4_ID: u32 = 11;
const TUPLE5_ID: u32 = 12;
const TUPLE6_ID: u32 = 13;
const TUPLE7_ID: u32 = 14;
const TUPLE8_ID: u32 = 15;

pub const FIRST_USER_CLASS_ID: u32 = TUPLE8_ID + 1;

/// The default module ID to assign to builtin types.
///
/// This ID is corrected using a `builtin class` definition.
const DEFAULT_BUILTIN_MODULE_ID: u32 = 0;

const INT_NAME: &str = "Int";
const FLOAT_NAME: &str = "Float";
const STRING_NAME: &str = "String";
const ARRAY_NAME: &str = "Array";
const BOOLEAN_NAME: &str = "Bool";
const NIL_NAME: &str = "Nil";
const BYTE_ARRAY_NAME: &str = "ByteArray";
const FUTURE_NAME: &str = "Future";

const TUPLE1_NAME: &str = "Tuple1";
const TUPLE2_NAME: &str = "Tuple2";
const TUPLE3_NAME: &str = "Tuple3";
const TUPLE4_NAME: &str = "Tuple4";
const TUPLE5_NAME: &str = "Tuple5";
const TUPLE6_NAME: &str = "Tuple6";
const TUPLE7_NAME: &str = "Tuple7";
const TUPLE8_NAME: &str = "Tuple8";

pub const STRING_MODULE: &str = "std::string";
pub const TO_STRING_TRAIT: &str = "ToString";
pub const TO_STRING_METHOD: &str = "to_string";

pub const CALL_METHOD: &str = "call";
pub const MAIN_CLASS: &str = "Main";
pub const MAIN_METHOD: &str = "main";

pub const DROP_MODULE: &str = "std::drop";
pub const DROP_TRAIT: &str = "Drop";

pub const DROP_METHOD: &str = "drop";
pub const DROPPER_METHOD: &str = "$dropper";
pub const ASYNC_DROPPER_METHOD: &str = "$async_dropper";

pub const CLONE_MODULE: &str = "std::clone";
pub const CLONE_TRAIT: &str = "Clone";
pub const CLONE_METHOD: &str = "clone";

pub const ENUM_TAG_FIELD: &str = "tag";
pub const ENUM_TAG_INDEX: usize = 0;

/// The maximum number of enum variants that can be defined in a single class.
pub const VARIANTS_LIMIT: usize = u16::MAX as usize;

/// The maximum number of fields a class can define.
pub const FIELDS_LIMIT: usize = u8::MAX as usize;

const MAX_FORMATTING_DEPTH: usize = 8;

/// The maximum recursion/depth to restrict ourselves to when inferring types or
/// checking if they are inferred.
///
/// In certain cases we may end up with cyclic types, where the cycles are
/// non-trivial (e.g. `A -> B -> C -> D -> A`). To prevent runaway recursion we
/// limit such operations to a certain depth.
///
/// The depth here is sufficiently large that no sane program should run into
/// it, but we also won't blow the stack.
const MAX_TYPE_DEPTH: usize = 64;

pub fn format_type<T: FormatType>(db: &Database, typ: T) -> String {
    TypeFormatter::new(db, None, None).format(typ)
}

pub fn format_type_with_self<T: FormatType>(
    db: &Database,
    self_type: TypeId,
    typ: T,
) -> String {
    TypeFormatter::new(db, Some(self_type), None).format(typ)
}

pub fn format_type_with_context<T: FormatType>(
    db: &Database,
    context: &TypeContext,
    typ: T,
) -> String {
    TypeFormatter::new(
        db,
        Some(context.self_type),
        Some(&context.type_arguments),
    )
    .format(typ)
}

#[derive(Copy, Clone)]
pub enum CompilerMacro {
    FutureGet,
    FutureGetFor,
    StringClone,
    Moved,
    PanicThrown,
    Strings,
}

impl CompilerMacro {
    pub fn name(self) -> &'static str {
        match self {
            CompilerMacro::FutureGet => "future_get",
            CompilerMacro::FutureGetFor => "future_get_for",
            CompilerMacro::StringClone => "string_clone",
            CompilerMacro::Moved => "moved",
            CompilerMacro::PanicThrown => "panic_thrown",
            CompilerMacro::Strings => "strings",
        }
    }
}

/// A buffer for formatting type names.
///
/// We use a simple wrapper around a String so we can more easily change the
/// implementation in the future if necessary.
pub struct TypeFormatter<'a> {
    db: &'a Database,
    self_type: Option<TypeId>,
    type_arguments: Option<&'a TypeArguments>,
    buffer: String,
    depth: usize,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(
        db: &'a Database,
        self_type: Option<TypeId>,
        type_arguments: Option<&'a TypeArguments>,
    ) -> Self {
        Self { db, self_type, type_arguments, buffer: String::new(), depth: 0 }
    }

    pub fn format<T: FormatType>(mut self, typ: T) -> String {
        typ.format_type(&mut self);
        self.buffer
    }

    fn descend<F: FnOnce(&mut TypeFormatter)>(&mut self, block: F) {
        if self.depth == MAX_FORMATTING_DEPTH {
            self.write("...");
        } else {
            self.depth += 1;

            block(self);

            self.depth -= 1;
        }
    }

    fn write(&mut self, thing: &str) {
        self.buffer.push_str(thing);
    }

    /// If a uni/ref/mut value wraps a type parameter, and that parameter is
    /// assigned another value with ownership, you can end up with e.g.
    /// `ref mut T` or `uni uni T`. This method provides a simple way of
    /// preventing this from happening, without complicating the type formatting
    /// process.
    fn write_ownership(&mut self, thing: &str) {
        if !self.buffer.ends_with(thing) {
            self.write(thing);
        }
    }

    fn type_arguments(
        &mut self,
        parameters: &[TypeParameterId],
        arguments: &TypeArguments,
    ) {
        for (index, &param) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            match arguments.get(param) {
                Some(TypeRef::Placeholder(id))
                    if id.value(self.db).is_none() =>
                {
                    // Placeholders without values aren't useful to show to the
                    // developer, so we show the type parameter instead.
                    //
                    // The parameter itself may be assigned a value through the
                    // type context (e.g. when a type is nested such as
                    // `Array[Array[T]]`), and we don't want to display that
                    // assignment as it's only to be used for the outer most
                    // type. As such, we don't use format_type() here.
                    param.format_type_without_argument(self);
                }
                Some(typ) => typ.format_type(self),
                _ => param.format_type(self),
            }
        }
    }

    fn arguments(&mut self, arguments: &Arguments, include_name: bool) {
        if arguments.len() == 0 {
            return;
        }

        self.write(" (");

        for (index, arg) in arguments.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            if include_name {
                self.write(&arg.name);
                self.write(": ");
            }

            arg.value_type.format_type(self);
        }

        self.write(")");
    }

    fn throw_type(&mut self, typ: TypeRef) {
        if typ.is_never(self.db) {
            return;
        }

        match typ {
            TypeRef::Placeholder(id) if id.value(self.db).is_none() => {}
            _ => {
                self.write(" !! ");
                typ.format_type(self);
            }
        }
    }

    fn return_type(&mut self, typ: TypeRef) {
        match typ {
            TypeRef::Placeholder(id) if id.value(self.db).is_none() => {}
            _ if typ == TypeRef::nil() => {}
            _ => {
                self.write(" -> ");
                typ.format_type(self);
            }
        }
    }
}

/// A type of which the name can be formatted into something human-readable.
pub trait FormatType {
    fn format_type(&self, buffer: &mut TypeFormatter);
}

/// A placeholder for a type that has yet to be inferred.
pub struct TypePlaceholder {
    /// The value assigned to this placeholder.
    ///
    /// This is wrapped in a Cell so we don't need a mutable borrow to the
    /// Database when updating a placeholder. This in turn is needed because
    /// type-checking functions expect/depend on an immutable database, and
    /// can't work with a mutable one (e.g. due to having to borrow multiple
    /// fields).
    value: Cell<TypeRef>,

    /// When `self` is assigned a value, these placeholders are assigned the
    /// same value.
    ///
    /// One place where this is needed is array literals with multiple values:
    /// only the first placeholder/value is stored in the Array type, so further
    /// inferring of that type doesn't affect values at index 1, 2, etc. By
    /// recording `ours` here, updating `id` also updates `ours`, without
    /// introducing a placeholder cycle.
    depending: Vec<TypePlaceholderId>,
}

impl TypePlaceholder {
    fn alloc(db: &mut Database) -> TypePlaceholderId {
        let id = db.type_placeholders.len();
        let typ = TypePlaceholder {
            value: Cell::new(TypeRef::Unknown),
            depending: Vec::new(),
        };

        db.type_placeholders.push(typ);
        TypePlaceholderId(id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypePlaceholderId(usize);

impl TypePlaceholderId {
    pub fn value(self, db: &Database) -> Option<TypeRef> {
        match self.get(db).value.get() {
            TypeRef::Unknown => None,
            value => Some(value),
        }
    }

    fn add_depending(self, db: &mut Database, placeholder: TypePlaceholderId) {
        self.get_mut(db).depending.push(placeholder);
    }

    fn assign(self, db: &Database, value: TypeRef) {
        // Assigning placeholders to themselves creates cycles that aren't
        // useful, so we ignore those.
        if let TypeRef::Placeholder(id) = value {
            if id.0 == self.0 {
                return;
            }
        }

        self.get(db).value.set(value);

        for &id in &self.get(db).depending {
            id.get(db).value.set(value);
        }
    }

    fn get(self, db: &Database) -> &TypePlaceholder {
        &db.type_placeholders[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut TypePlaceholder {
        &mut db.type_placeholders[self.0]
    }
}

impl FormatType for TypePlaceholderId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        if let Some(value) = self.value(buffer.db) {
            value.format_type(buffer);
        } else {
            buffer.write("?");
        }
    }
}

/// A collection of values needed when checking and substituting types.
#[derive(Clone)]
pub struct TypeContext {
    /// The type of `Self`.
    ///
    /// This isn't the same type as `self`: `Self` is a new instance of a type,
    /// whereas `self` is the receiver. Consider this example:
    ///
    ///     class A {
    ///       fn foo -> Self {}
    ///     }
    ///
    /// Within `foo`, the type of `self` is `ref A`, but the type of `Self` is
    /// `A`.
    pub self_type: TypeId,

    /// The type arguments available to this context.
    ///
    /// When type-checking a method call, this table contains the type
    /// parameters and values of both the receiver and the method itself.
    pub type_arguments: TypeArguments,

    /// The nesting/recursion depth when e.g. inferring a type.
    ///
    /// This value is used to prevent runaway recursion that can occur when
    /// dealing with (complex) cyclic types.
    depth: usize,
}

impl TypeContext {
    pub fn new(self_type_id: TypeId) -> Self {
        Self {
            self_type: self_type_id,
            type_arguments: TypeArguments::new(),
            depth: 0,
        }
    }

    pub fn for_class_instance(
        db: &Database,
        self_type: TypeId,
        instance: ClassInstance,
    ) -> Self {
        let type_arguments = if instance.instance_of().is_generic(db) {
            instance.type_arguments(db).clone()
        } else {
            TypeArguments::new()
        };

        Self { self_type, type_arguments, depth: 0 }
    }

    pub fn with_arguments(
        self_type_id: TypeId,
        type_arguments: TypeArguments,
    ) -> Self {
        Self { self_type: self_type_id, type_arguments, depth: 0 }
    }
}

/// A type parameter for a method or class.
pub struct TypeParameter {
    /// The name of the type parameter.
    name: String,

    /// The traits that must be implemented before a type can be assigned to
    /// this type parameter.
    requirements: Vec<TraitInstance>,
}

impl TypeParameter {
    pub fn alloc(db: &mut Database, name: String) -> TypeParameterId {
        let id = db.type_parameters.len();
        let typ = TypeParameter::new(name);

        db.type_parameters.push(typ);
        TypeParameterId(id)
    }

    fn new(name: String) -> Self {
        Self { name, requirements: Vec::new() }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TypeParameterId(usize);

impl TypeParameterId {
    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn add_requirements(
        self,
        db: &mut Database,
        mut requirements: Vec<TraitInstance>,
    ) {
        self.get_mut(db).requirements.append(&mut requirements);
    }

    pub fn requirements(self, db: &Database) -> Vec<TraitInstance> {
        self.get(db).requirements.clone()
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        let typ = self.get(db);

        for &req in &typ.requirements {
            if let Some(m) = req.method(db, name) {
                return Some(m);
            }
        }

        None
    }

    fn all_requirements_met(
        self,
        db: &mut Database,
        mut func: impl FnMut(&mut Database, TraitInstance) -> bool,
    ) -> bool {
        self.get(db).requirements.clone().into_iter().all(|r| func(db, r))
    }

    fn get(self, db: &Database) -> &TypeParameter {
        &db.type_parameters[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut TypeParameter {
        &mut db.type_parameters[self.0]
    }

    fn type_check(
        self,
        db: &mut Database,
        with: TypeId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match with {
            TypeId::TraitInstance(theirs) => self
                .type_check_with_trait_instance(db, theirs, context, subtyping),
            TypeId::TypeParameter(theirs) => self
                .type_check_with_type_parameter(db, theirs, context, subtyping),
            _ => false,
        }
    }

    fn type_check_with_type_parameter(
        self,
        db: &mut Database,
        with: TypeParameterId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        with.all_requirements_met(db, |db, req| {
            self.type_check_with_trait_instance(db, req, context, subtyping)
        })
    }

    fn type_check_with_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        self.get(db).requirements.clone().into_iter().any(|req| {
            req.type_check_with_trait_instance(
                db, instance, None, context, subtyping,
            )
        })
    }

    fn as_rigid_type(self, bounds: &TypeBounds) -> TypeId {
        TypeId::RigidTypeParameter(bounds.get(self).unwrap_or(self))
    }

    fn as_owned_rigid(self) -> TypeRef {
        TypeRef::Owned(TypeId::RigidTypeParameter(self))
    }

    fn format_type_without_argument(&self, buffer: &mut TypeFormatter) {
        let param = self.get(buffer.db);

        buffer.write(&param.name);

        if !param.requirements.is_empty() {
            buffer.write(": ");

            for (index, req) in param.requirements.iter().enumerate() {
                if index > 0 {
                    buffer.write(" + ");
                }

                req.format_type(buffer);
            }
        }
    }
}

impl FormatType for TypeParameterId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        // Formatting type parameters is a bit tricky, as they may be assigned
        // to themselves directly or through a placeholder. The below code isn't
        // going to win any awards, but it should ensure we don't blow the stack
        // when trying to format recursive type parameters, such as
        // `T -> placeholder -> T`.

        if let Some(arg) = buffer.type_arguments.and_then(|a| a.get(*self)) {
            if let TypeRef::Placeholder(p) = arg {
                match p.value(buffer.db) {
                    Some(t) if t.as_type_parameter() == Some(*self) => {
                        self.format_type_without_argument(buffer)
                    }
                    Some(t) => t.format_type(buffer),
                    None => self.format_type_without_argument(buffer),
                }

                return;
            }

            if arg.as_type_parameter() == Some(*self) {
                self.format_type_without_argument(buffer);
                return;
            }

            arg.format_type(buffer);
        } else {
            self.format_type_without_argument(buffer);
        };
    }
}

/// Type parameters and the types assigned to them.
#[derive(Clone)]
pub struct TypeArguments {
    /// We use a HashMap as parameters can be assigned in any order, and some
    /// may not be assigned at all.
    mapping: HashMap<TypeParameterId, TypeRef>,
}

impl TypeArguments {
    fn rigid(db: &mut Database, index: u32, bounds: &TypeBounds) -> Self {
        let mut new_args = Self::new();

        for (param, value) in db.type_arguments[index as usize].pairs() {
            new_args.assign(param, value.as_rigid_type(db, bounds));
        }

        new_args
    }

    pub fn new() -> Self {
        Self { mapping: HashMap::default() }
    }

    pub fn assign(&mut self, parameter: TypeParameterId, value: TypeRef) {
        self.mapping.insert(parameter, value);
    }

    pub fn get(&self, parameter: TypeParameterId) -> Option<TypeRef> {
        self.mapping.get(&parameter).cloned()
    }

    pub fn pairs(&self) -> Vec<(TypeParameterId, TypeRef)> {
        self.mapping.iter().map(|(&a, &b)| (a, b)).collect()
    }

    pub fn copy_into(&self, other: &mut Self) {
        for (&key, &value) in &self.mapping {
            other.assign(key, value);
        }
    }

    pub fn move_into(self, other: &mut Self) {
        for (key, value) in self.mapping {
            other.assign(key, value);
        }
    }

    pub fn copy_assigned_into(
        &self,
        parameters: Vec<TypeParameterId>,
        target: &mut Self,
    ) {
        for param in parameters {
            if let Some(value) = self.get(param) {
                target.assign(param, value);
            }
        }
    }

    fn assigned_or_placeholders(
        &self,
        db: &mut Database,
        parameters: Vec<TypeParameterId>,
    ) -> Self {
        let mut new_args = Self::new();

        for param in parameters {
            if let Some(val) = self.get(param) {
                new_args.assign(param, val);
            } else {
                new_args.assign(param, TypeRef::placeholder(db));
            }
        }

        new_args
    }
}

/// An Inko trait.
pub struct Trait {
    name: String,
    module: ModuleId,
    implemented_by: Vec<ClassId>,
    visibility: Visibility,
    type_parameters: IndexMap<String, TypeParameterId>,
    required_traits: Vec<TraitInstance>,
    default_methods: IndexMap<String, MethodId>,
    required_methods: IndexMap<String, MethodId>,

    /// The type arguments inherited from any of the required traits.
    ///
    /// Traits may require generic traits, which in turn can require other
    /// generic traits, and so on. When we have an instance of trait `T`, we may
    /// end up calling a method from one of its ancestors. If that method
    /// returns a type parameter, we want to map it to the proper type. Consider
    /// this chain of types:
    ///
    ///     trait A[P1] {
    ///       fn foo -> P1
    ///     }
    ///
    ///     trait B[P2]: A[P2] {}
    ///     trait C[P3]: B[P3] {}
    ///
    /// Given an instance of `C[Int]`, `foo` should return `Int` as well, even
    /// though `P1` isn't explicitly assigned a value in `C[Int]`. Since walking
    /// the entire trait requirement chain every lookup is expensive, we store
    /// the inherited arguments. In the above example that means this structure
    /// contains the following mappings:
    ///
    ///     P2 -> P3
    ///     P1 -> P2
    ///
    /// Whenever we need to lookup type parameter assignments for an instance of
    /// `C`, we just copy this structure and use it for lookups and updates
    /// accordingly.
    ///
    /// Since most traits don't require many other traits, the overhead of this
    /// should be minimal, and less compared to walking requirement chains when
    /// performing lookups.
    inherited_type_arguments: TypeArguments,
}

impl Trait {
    pub fn alloc(
        db: &mut Database,
        name: String,
        module: ModuleId,
        visibility: Visibility,
    ) -> TraitId {
        assert!(db.traits.len() <= u32::MAX as usize);

        let id = db.traits.len() as u32;
        let trait_type = Trait::new(name, module, visibility);

        db.traits.push(trait_type);
        TraitId(id)
    }

    fn new(name: String, module: ModuleId, visibility: Visibility) -> Self {
        Self {
            name,
            module,
            visibility,
            implemented_by: Vec::new(),
            type_parameters: IndexMap::new(),
            required_traits: Vec::new(),
            default_methods: IndexMap::new(),
            required_methods: IndexMap::new(),
            inherited_type_arguments: TypeArguments::new(),
        }
    }

    fn is_generic(&self) -> bool {
        self.type_parameters.len() > 0
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TraitId(pub u32);

impl TraitId {
    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn type_parameters(self, db: &Database) -> Vec<TypeParameterId> {
        self.get(db).type_parameters.values().clone()
    }

    pub fn required_traits(self, db: &Database) -> Vec<TraitInstance> {
        self.get(db).required_traits.clone()
    }

    pub fn required_methods(self, db: &Database) -> Vec<MethodId> {
        self.get(db).required_methods.values().clone()
    }

    pub fn default_methods(self, db: &Database) -> Vec<MethodId> {
        self.get(db).default_methods.values().clone()
    }

    pub fn add_required_trait(
        self,
        db: &mut Database,
        requirement: TraitInstance,
    ) {
        let mut base =
            requirement.instance_of.get(db).inherited_type_arguments.clone();

        if requirement.instance_of.is_generic(db) {
            requirement.type_arguments(db).copy_into(&mut base);
        }

        let self_typ = self.get_mut(db);

        base.move_into(&mut self_typ.inherited_type_arguments);
        self_typ.required_traits.push(requirement);
    }

    pub fn method_exists(self, db: &Database, name: &str) -> bool {
        self.get(db).default_methods.contains_key(name)
            || self.get(db).required_methods.contains_key(name)
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        let typ = self.get(db);

        if let Some(&id) = typ
            .default_methods
            .get(name)
            .or_else(|| typ.required_methods.get(name))
        {
            return Some(id);
        }

        for &req in &typ.required_traits {
            if let Some(id) = req.method(db, name) {
                return Some(id);
            }
        }

        None
    }

    pub fn add_default_method(
        self,
        db: &mut Database,
        name: String,
        method: MethodId,
    ) {
        self.get_mut(db).default_methods.insert(name, method);
    }

    pub fn add_required_method(
        self,
        db: &mut Database,
        name: String,
        method: MethodId,
    ) {
        self.get_mut(db).required_methods.insert(name, method);
    }

    pub fn is_generic(self, db: &Database) -> bool {
        self.get(db).is_generic()
    }

    pub fn number_of_type_parameters(&self, db: &Database) -> usize {
        self.get(db).type_parameters.len()
    }

    pub fn type_parameter_exists(&self, db: &Database, name: &str) -> bool {
        self.get(db).type_parameters.contains_key(name)
    }

    pub fn new_type_parameter(
        &self,
        db: &mut Database,
        name: String,
    ) -> TypeParameterId {
        let param = TypeParameter::alloc(db, name.clone());

        self.get_mut(db).type_parameters.insert(name, param);
        param
    }

    fn is_public(self, db: &Database) -> bool {
        self.get(db).visibility == Visibility::Public
    }

    pub fn is_private(self, db: &Database) -> bool {
        !self.is_public(db)
    }

    fn module(self, db: &Database) -> ModuleId {
        self.get(db).module
    }

    fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        self.get(db)
            .type_parameters
            .get(name)
            .map(|&id| Symbol::TypeParameter(id))
    }

    fn get(self, db: &Database) -> &Trait {
        &db.traits[self.0 as usize]
    }

    fn get_mut(self, db: &mut Database) -> &mut Trait {
        &mut db.traits[self.0 as usize]
    }
}

impl FormatType for TraitId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
    }
}

/// An instance of a trait, along with its type arguments in case the trait is
/// generic.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitInstance {
    /// The ID of the trait we're an instance of.
    instance_of: TraitId,

    /// The index to the TypeArguments structure that belongs to this trait
    /// instance.
    ///
    /// If the trait is a regular trait, this ID is always 0 and shouldn't be
    /// used.
    type_arguments: u32,
}

impl TraitInstance {
    /// Returns an instance to use as the type of `Self` in required and default
    /// methods.
    pub fn for_self_type(
        db: &mut Database,
        instance_of: TraitId,
        bounds: &TypeBounds,
    ) -> Self {
        if instance_of.is_generic(db) {
            let mut arguments = TypeArguments::new();

            for param in instance_of.type_parameters(db) {
                arguments.assign(
                    param,
                    bounds.get(param).unwrap_or(param).as_owned_rigid(),
                );
            }

            Self::generic(db, instance_of, arguments)
        } else {
            Self::new(instance_of)
        }
    }

    pub fn new(instance_of: TraitId) -> Self {
        Self { instance_of, type_arguments: 0 }
    }

    pub fn generic(
        db: &mut Database,
        instance_of: TraitId,
        arguments: TypeArguments,
    ) -> Self {
        assert!(db.type_arguments.len() <= u32::MAX as usize);

        let type_args_id = db.type_arguments.len() as u32;

        db.type_arguments.push(arguments);
        TraitInstance { instance_of, type_arguments: type_args_id }
    }

    pub fn instance_of(self) -> TraitId {
        self.instance_of
    }

    pub fn type_arguments(self, db: &Database) -> &TypeArguments {
        &db.type_arguments[self.type_arguments as usize]
    }

    pub fn copy_new_arguments_from(
        self,
        db: &mut Database,
        from: &TypeArguments,
    ) {
        if !self.instance_of.is_generic(db) {
            return;
        }

        let params = self.instance_of.type_parameters(db);
        let targs = &mut db.type_arguments[self.type_arguments as usize];

        from.copy_assigned_into(params, targs);
    }

    pub fn copy_type_arguments_into(
        self,
        db: &Database,
        target: &mut TypeArguments,
    ) {
        if !self.instance_of.is_generic(db) {
            return;
        }

        self.type_arguments(db).copy_into(target);
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        self.instance_of.method(db, name)
    }

    fn type_check(
        self,
        db: &mut Database,
        with: TypeId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match with {
            TypeId::TraitInstance(ins) => self.type_check_with_trait_instance(
                db, ins, None, context, subtyping,
            ),
            TypeId::TypeParameter(id) => {
                id.all_requirements_met(db, |db, req| {
                    self.type_check_with_trait_instance(
                        db, req, None, context, subtyping,
                    )
                })
            }
            _ => false,
        }
    }

    fn type_check_with_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        arguments: Option<&TypeArguments>,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        if self.instance_of != instance.instance_of {
            return if subtyping {
                self.instance_of
                    .get(db)
                    .required_traits
                    .clone()
                    .into_iter()
                    .any(|req| {
                        req.type_check_with_trait_instance(
                            db, instance, None, context, subtyping,
                        )
                    })
            } else {
                false
            };
        }

        let our_trait = self.instance_of.get(db);

        if !our_trait.is_generic() {
            return true;
        }

        let our_args = self.type_arguments(db).clone();
        let their_args = instance.type_arguments(db).clone();

        // If additional type arguments are given (e.g. when comparing a generic
        // class instance to a trait), we need to remap the implementation
        // arguments accordingly. This way if `Box[T]` implements `Iter[T]`, for
        // a `Box[Int]` we produce a `Iter[Int]` rather than an `Iter[T]`.
        if let Some(args) = arguments {
            our_trait.type_parameters.values().clone().into_iter().all(
                |param| {
                    our_args
                        .get(param)
                        .zip(their_args.get(param))
                        .map(|(ours, theirs)| {
                            ours.as_type_parameter()
                                .and_then(|id| args.get(id))
                                .unwrap_or(ours)
                                .type_check(db, theirs, context, subtyping)
                        })
                        .unwrap_or(false)
                },
            )
        } else {
            our_trait.type_parameters.values().clone().into_iter().all(
                |param| {
                    our_args
                        .get(param)
                        .zip(their_args.get(param))
                        .map(|(ours, theirs)| {
                            ours.type_check(db, theirs, context, subtyping)
                        })
                        .unwrap_or(false)
                },
            )
        }
    }

    fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        self.instance_of.named_type(db, name)
    }

    fn implements_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        context: &mut TypeContext,
    ) -> bool {
        self.instance_of.get(db).required_traits.clone().into_iter().any(
            |req| {
                req.type_check_with_trait_instance(
                    db, instance, None, context, true,
                )
            },
        )
    }

    fn implements_trait_id(self, db: &Database, trait_id: TraitId) -> bool {
        self.instance_of
            .get(db)
            .required_traits
            .iter()
            .any(|req| req.implements_trait_id(db, trait_id))
    }

    fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        if !self.instance_of.get(db).is_generic() {
            return self;
        }

        let new_args = TypeArguments::rigid(db, self.type_arguments, bounds);

        TraitInstance::generic(db, self.instance_of, new_args)
    }

    fn inferred(
        self,
        db: &mut Database,
        context: &mut TypeContext,
        immutable: bool,
    ) -> Self {
        if !self.instance_of.is_generic(db) {
            return self;
        }

        let mut new_args = TypeArguments::new();

        for (arg, val) in self.type_arguments(db).pairs() {
            new_args.assign(arg, val.inferred(db, context, immutable));
        }

        Self::generic(db, self.instance_of, new_args)
    }
}

impl FormatType for TraitInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            buffer.write(&ins_of.name);

            if ins_of.type_parameters.len() > 0 {
                let params = ins_of.type_parameters.values();
                let args = self.type_arguments(buffer.db);

                buffer.write("[");
                buffer.type_arguments(params, args);
                buffer.write("]");
            }
        });
    }
}

/// A field for a class.
pub struct Field {
    index: usize,
    name: String,
    value_type: TypeRef,
    visibility: Visibility,
    module: ModuleId,
}

impl Field {
    pub fn alloc(
        db: &mut Database,
        name: String,
        index: usize,
        value_type: TypeRef,
        visibility: Visibility,
        module: ModuleId,
    ) -> FieldId {
        let id = db.fields.len();

        db.fields.push(Field { name, index, value_type, visibility, module });
        FieldId(id)
    }
}

/// An ID to a field.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct FieldId(pub usize);

impl FieldId {
    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn index(self, db: &Database) -> usize {
        self.get(db).index
    }

    pub fn value_type(self, db: &Database) -> TypeRef {
        self.get(db).value_type
    }

    pub fn is_public(self, db: &Database) -> bool {
        self.get(db).visibility == Visibility::Public
    }

    pub fn is_visible_to(self, db: &Database, module: ModuleId) -> bool {
        let field = self.get(db);

        match field.visibility {
            Visibility::Public => true,
            Visibility::Private => field.module == module,
            // TypePrivate fields can only be accessed using the `@name` syntax,
            // which in turn is only available inside a class, thus not needing
            // any extra checks.
            Visibility::TypePrivate => false,
        }
    }

    fn get(self, db: &Database) -> &Field {
        &db.fields[self.0]
    }
}

/// Additional requirements for type parameters inside a trait implementation of
/// method.
///
/// Additional bounds are set using the `when` keyword like so:
///
///     impl Debug[T] for Array when T: X { ... }
///
/// This structure maps the original type parameters (`T` in this case) to type
/// parameters created for the bounds. These new type parameters have their
/// requirements set to the union of the original type parameter's requirements,
/// and the requirements specified in the bounds. In other words, if the
/// original parameter is defined as `T: A` and the bounds specify `T: B`, this
/// structure maps `T: A` to `T: A + B`.
#[derive(Clone)]
pub struct TypeBounds {
    mapping: HashMap<TypeParameterId, TypeParameterId>,
}

impl TypeBounds {
    pub fn new() -> Self {
        Self { mapping: HashMap::default() }
    }

    pub fn set(&mut self, parameter: TypeParameterId, bounds: TypeParameterId) {
        self.mapping.insert(parameter, bounds);
    }

    pub fn get(&self, parameter: TypeParameterId) -> Option<TypeParameterId> {
        self.mapping.get(&parameter).cloned()
    }
}

/// An implementation of a trait, with (optionally) additional bounds for the
/// implementation.
#[derive(Clone)]
pub struct TraitImplementation {
    pub instance: TraitInstance,
    pub bounds: TypeBounds,
}

/// A single variant defined in a enum class.
pub struct Variant {
    /// The ID of the variant local to its class.
    pub id: u16,

    /// The name of the variant.
    pub name: String,

    /// The member types of this variant.
    ///
    /// For a variant defined as `Foo(Int, Int)`, this would be `[Int, Int]`.
    pub members: Vec<TypeRef>,
}

impl Variant {
    pub fn alloc(
        db: &mut Database,
        id: u16,
        name: String,
        members: Vec<TypeRef>,
    ) -> VariantId {
        let global_id = db.variants.len();

        db.variants.push(Variant { id, name, members });
        VariantId(global_id)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct VariantId(usize);

impl VariantId {
    pub fn id(self, db: &Database) -> u16 {
        self.get(db).id
    }

    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn members(self, db: &Database) -> Vec<TypeRef> {
        self.get(db).members.clone()
    }

    pub fn number_of_members(self, db: &Database) -> usize {
        self.get(db).members.len()
    }

    fn get(self, db: &Database) -> &Variant {
        &db.variants[self.0]
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum ClassKind {
    Async,
    Enum,
    Regular,
    Tuple,
}

impl ClassKind {
    pub fn is_async(self) -> bool {
        matches!(self, ClassKind::Async)
    }

    pub fn is_enum(self) -> bool {
        matches!(self, ClassKind::Enum)
    }

    pub fn is_regular(self) -> bool {
        matches!(self, ClassKind::Regular)
    }

    pub fn is_tuple(self) -> bool {
        matches!(self, ClassKind::Tuple)
    }
}

/// An Inko class as declared using the `class` keyword.
pub struct Class {
    kind: ClassKind,
    name: String,
    // A flag indicating the presence of a custom destructor.
    //
    // We store a flag for this so we can check for the presence of a destructor
    // without having to look up traits.
    destructor: bool,
    module: ModuleId,
    visibility: Visibility,
    fields: IndexMap<String, FieldId>,
    type_parameters: IndexMap<String, TypeParameterId>,
    methods: HashMap<String, MethodId>,
    implemented_traits: HashMap<TraitId, TraitImplementation>,
    variants: IndexMap<String, VariantId>,
}

impl Class {
    pub fn alloc(
        db: &mut Database,
        name: String,
        kind: ClassKind,
        visibility: Visibility,
        module: ModuleId,
    ) -> ClassId {
        assert!(db.classes.len() <= u32::MAX as usize);

        let id = db.classes.len() as u32;
        let class = Class::new(name, kind, visibility, module);

        db.classes.push(class);
        ClassId(id)
    }

    fn new(
        name: String,
        kind: ClassKind,
        visibility: Visibility,
        module: ModuleId,
    ) -> Self {
        Self {
            name,
            kind,
            visibility,
            destructor: false,
            fields: IndexMap::new(),
            type_parameters: IndexMap::new(),
            methods: HashMap::new(),
            implemented_traits: HashMap::new(),
            variants: IndexMap::new(),
            module,
        }
    }

    fn regular(name: String) -> Self {
        Self::new(
            name,
            ClassKind::Regular,
            Visibility::Public,
            ModuleId(DEFAULT_BUILTIN_MODULE_ID),
        )
    }

    fn tuple(name: String) -> Self {
        Self::new(
            name,
            ClassKind::Tuple,
            Visibility::Public,
            ModuleId(DEFAULT_BUILTIN_MODULE_ID),
        )
    }

    fn is_generic(&self) -> bool {
        self.type_parameters.len() > 0
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClassId(pub u32);

impl ClassId {
    pub fn int() -> ClassId {
        ClassId(INT_ID)
    }

    pub fn float() -> ClassId {
        ClassId(FLOAT_ID)
    }

    pub fn string() -> ClassId {
        ClassId(STRING_ID)
    }

    pub fn boolean() -> ClassId {
        ClassId(BOOLEAN_ID)
    }

    pub fn nil() -> ClassId {
        ClassId(NIL_ID)
    }

    pub fn future() -> ClassId {
        ClassId(FUTURE_ID)
    }

    pub fn array() -> ClassId {
        ClassId(ARRAY_ID)
    }

    pub fn byte_array() -> ClassId {
        ClassId(BYTE_ARRAY_ID)
    }

    pub fn tuple1() -> ClassId {
        ClassId(TUPLE1_ID)
    }

    pub fn tuple2() -> ClassId {
        ClassId(TUPLE2_ID)
    }

    pub fn tuple3() -> ClassId {
        ClassId(TUPLE3_ID)
    }

    pub fn tuple4() -> ClassId {
        ClassId(TUPLE4_ID)
    }

    pub fn tuple5() -> ClassId {
        ClassId(TUPLE5_ID)
    }

    pub fn tuple6() -> ClassId {
        ClassId(TUPLE6_ID)
    }

    pub fn tuple7() -> ClassId {
        ClassId(TUPLE7_ID)
    }

    pub fn tuple8() -> ClassId {
        ClassId(TUPLE8_ID)
    }

    pub fn tuple(len: usize) -> Option<ClassId> {
        match len {
            1 => Some(ClassId::tuple1()),
            2 => Some(ClassId::tuple2()),
            3 => Some(ClassId::tuple3()),
            4 => Some(ClassId::tuple4()),
            5 => Some(ClassId::tuple5()),
            6 => Some(ClassId::tuple6()),
            7 => Some(ClassId::tuple7()),
            8 => Some(ClassId::tuple8()),
            _ => None,
        }
    }

    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn kind(self, db: &Database) -> ClassKind {
        self.get(db).kind
    }

    pub fn type_parameters(self, db: &Database) -> Vec<TypeParameterId> {
        self.get(db).type_parameters.values().clone()
    }

    pub fn add_trait_implementation(
        self,
        db: &mut Database,
        implementation: TraitImplementation,
    ) {
        let trait_id = implementation.instance.instance_of();

        self.get_mut(db).implemented_traits.insert(trait_id, implementation);
        trait_id.get_mut(db).implemented_by.push(self);
    }

    pub fn trait_implementation(
        self,
        db: &Database,
        trait_type: TraitId,
    ) -> Option<&TraitImplementation> {
        self.get(db).implemented_traits.get(&trait_type)
    }

    pub fn new_variant(
        self,
        db: &mut Database,
        name: String,
        members: Vec<TypeRef>,
    ) -> VariantId {
        let id = self.get(db).variants.len() as u16;
        let variant = Variant::alloc(db, id, name.clone(), members);

        self.get_mut(db).variants.insert(name, variant);
        variant
    }

    pub fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        self.type_parameter(db, name).map(Symbol::TypeParameter)
    }

    pub fn type_parameter(
        self,
        db: &Database,
        name: &str,
    ) -> Option<TypeParameterId> {
        self.get(db).type_parameters.get(name).cloned()
    }

    pub fn field(self, db: &Database, name: &str) -> Option<FieldId> {
        self.get(db).fields.get(name).cloned()
    }

    pub fn field_by_index(
        self,
        db: &Database,
        index: usize,
    ) -> Option<FieldId> {
        self.get(db).fields.get_index(index).cloned()
    }

    pub fn field_names(self, db: &Database) -> Vec<String> {
        self.get(db).fields.keys().map(|k| k.to_string()).collect()
    }

    pub fn fields(self, db: &Database) -> Vec<FieldId> {
        self.get(db).fields.values().clone()
    }

    pub fn new_field(
        self,
        db: &mut Database,
        name: String,
        index: usize,
        value_type: TypeRef,
        visibility: Visibility,
        module: ModuleId,
    ) -> FieldId {
        let id = Field::alloc(
            db,
            name.clone(),
            index,
            value_type,
            visibility,
            module,
        );

        self.get_mut(db).fields.insert(name, id);
        id
    }

    pub fn is_generic(self, db: &Database) -> bool {
        self.get(db).is_generic()
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        self.get(db).methods.get(name).cloned()
    }

    pub fn method_exists(self, db: &Database, name: &str) -> bool {
        self.get(db).methods.get(name).is_some()
    }

    pub fn add_method(self, db: &mut Database, name: String, method: MethodId) {
        self.get_mut(db).methods.insert(name, method);
    }

    pub fn variant(self, db: &Database, name: &str) -> Option<VariantId> {
        self.get(db).variants.get(name).cloned()
    }

    pub fn variants(self, db: &Database) -> Vec<VariantId> {
        self.get(db).variants.values().clone()
    }

    pub fn number_of_variants(self, db: &Database) -> usize {
        self.get(db).variants.len()
    }

    pub fn number_of_fields(self, db: &Database) -> usize {
        self.get(db).fields.len()
    }

    pub fn number_of_methods(self, db: &Database) -> usize {
        self.get(db).methods.len()
    }

    pub fn enum_fields(self, db: &Database) -> Vec<FieldId> {
        let obj = self.get(db);

        if obj.kind == ClassKind::Enum {
            // The first value is the tag, so we skip it.
            obj.fields.values()[1..].into()
        } else {
            Vec::new()
        }
    }

    pub fn is_public(self, db: &Database) -> bool {
        self.get(db).visibility == Visibility::Public
    }

    pub fn is_private(self, db: &Database) -> bool {
        !self.is_public(db)
    }

    pub fn set_module(self, db: &mut Database, module: ModuleId) {
        self.get_mut(db).module = module;
    }

    pub fn module(self, db: &Database) -> ModuleId {
        self.get(db).module
    }

    pub fn number_of_type_parameters(self, db: &Database) -> usize {
        self.get(db).type_parameters.len()
    }

    pub fn type_parameter_exists(self, db: &Database, name: &str) -> bool {
        self.get(db).type_parameters.contains_key(name)
    }

    pub fn new_type_parameter(
        self,
        db: &mut Database,
        name: String,
    ) -> TypeParameterId {
        let param = TypeParameter::alloc(db, name.clone());

        self.get_mut(db).type_parameters.insert(name, param);
        param
    }

    pub fn mark_as_having_destructor(self, db: &mut Database) {
        self.get_mut(db).destructor = true;
    }

    pub fn has_destructor(self, db: &Database) -> bool {
        self.get(db).destructor
    }

    fn get(self, db: &Database) -> &Class {
        &db.classes[self.0 as usize]
    }

    fn get_mut(self, db: &mut Database) -> &mut Class {
        &mut db.classes[self.0 as usize]
    }
}

impl FormatType for ClassId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
    }
}

/// An instance of a class, along with its type arguments in case the class is
/// generic.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClassInstance {
    /// The ID of the class we're an instance of.
    instance_of: ClassId,

    /// The index to the TypeArguments structure that belongs to this class
    /// instance.
    ///
    /// If the class is a regular class, this ID is always 0 and shouldn't be
    /// used.
    type_arguments: u32,
}

impl ClassInstance {
    /// Returns a class instance to use as the type of `Self` in instance
    /// methods.
    pub fn for_instance_self_type(
        db: &mut Database,
        instance_of: ClassId,
        bounds: &TypeBounds,
    ) -> Self {
        if instance_of.is_generic(db) {
            let mut arguments = TypeArguments::new();

            for param in instance_of.type_parameters(db) {
                arguments.assign(
                    param,
                    bounds.get(param).unwrap_or(param).as_owned_rigid(),
                );
            }

            Self::generic(db, instance_of, arguments)
        } else {
            Self::new(instance_of)
        }
    }

    /// Returns a class instance to use as the type of `Self` in static methods.
    pub fn for_static_self_type(
        db: &mut Database,
        instance_of: ClassId,
    ) -> Self {
        if instance_of.is_generic(db) {
            let mut arguments = TypeArguments::new();

            for param in instance_of.type_parameters(db) {
                arguments.assign(param, param.as_owned_rigid());
            }

            Self::generic(db, instance_of, arguments)
        } else {
            Self::new(instance_of)
        }
    }

    pub fn new(instance_of: ClassId) -> Self {
        Self { instance_of, type_arguments: 0 }
    }

    pub fn generic(
        db: &mut Database,
        instance_of: ClassId,
        arguments: TypeArguments,
    ) -> Self {
        assert!(db.type_arguments.len() <= u32::MAX as usize);

        let args_id = db.type_arguments.len() as u32;

        db.type_arguments.push(arguments);
        ClassInstance { instance_of, type_arguments: args_id }
    }

    pub fn generic_with_types(
        db: &mut Database,
        instance_of: ClassId,
        types: Vec<TypeRef>,
    ) -> Self {
        let mut args = TypeArguments::new();

        for (index, param) in
            instance_of.type_parameters(db).into_iter().enumerate()
        {
            args.assign(
                param,
                types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| TypeRef::placeholder(db)),
            );
        }

        Self::generic(db, instance_of, args)
    }

    pub fn generic_with_placeholders(
        db: &mut Database,
        instance_of: ClassId,
    ) -> Self {
        let mut args = TypeArguments::new();

        for param in instance_of.type_parameters(db) {
            args.assign(param, TypeRef::placeholder(db));
        }

        Self::generic(db, instance_of, args)
    }

    pub fn instance_of(self) -> ClassId {
        self.instance_of
    }

    pub fn type_arguments(self, db: &Database) -> &TypeArguments {
        &db.type_arguments[self.type_arguments as usize]
    }

    pub fn copy_new_arguments_from(
        self,
        db: &mut Database,
        from: &TypeArguments,
    ) {
        if !self.instance_of.is_generic(db) {
            return;
        }

        let params = self.instance_of.type_parameters(db);
        let targs = &mut db.type_arguments[self.type_arguments as usize];

        from.copy_assigned_into(params, targs);
    }

    pub fn copy_type_arguments_into(
        self,
        db: &Database,
        target: &mut TypeArguments,
    ) {
        if !self.instance_of.is_generic(db) {
            return;
        }

        self.type_arguments(db).copy_into(target);
    }

    pub fn type_check_with_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        if !subtyping {
            return false;
        }

        let trait_impl = if let Some(found) = self
            .instance_of
            .trait_implementation(db, instance.instance_of)
            .cloned()
        {
            found
        } else {
            return false;
        };

        let mut trait_instance = trait_impl.instance;

        if self.instance_of.is_generic(db)
            && trait_instance.instance_of.is_generic(db)
        {
            // The generic trait implementation may refer to (or contain a type
            // that refers) to a type parameter defined in our class. If we end
            // up comparing such a type parameter, we must compare its assigned
            // value instead if there is any.
            //
            // To achieve this we must first expose the type parameter
            // assignments in the context, then infer the trait instance into a
            // type that uses those assignments (if needed).
            self.type_arguments(db).copy_into(&mut context.type_arguments);
            trait_instance = trait_instance.inferred(db, context, false);
        }

        let args = if self.instance_of.is_generic(db) {
            let args = self.type_arguments(db).clone();
            let available =
                trait_impl.bounds.mapping.into_iter().all(|(orig, bound)| {
                    args.get(orig).map_or(false, |t| {
                        t.is_compatible_with_type_parameter(db, bound, context)
                    })
                });

            if !available {
                return false;
            }

            Some(args)
        } else {
            None
        };

        trait_instance.type_check_with_trait_instance(
            db,
            instance,
            args.as_ref(),
            context,
            subtyping,
        )
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        self.instance_of.method(db, name)
    }

    pub fn ordered_type_arguments(self, db: &Database) -> Vec<TypeRef> {
        let params = self.instance_of.type_parameters(db);
        let args = self.type_arguments(db);

        params
            .into_iter()
            .map(|p| args.get(p).unwrap_or(TypeRef::Unknown))
            .collect()
    }

    fn type_check(
        self,
        db: &mut Database,
        with: TypeId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match with {
            TypeId::ClassInstance(ins) => {
                if self.instance_of != ins.instance_of {
                    return false;
                }

                let our_class = self.instance_of.get(db);

                if !our_class.is_generic() {
                    return true;
                }

                let our_args = self.type_arguments(db).clone();
                let their_args = ins.type_arguments(db).clone();

                if our_args.mapping.is_empty() {
                    // Empty types are compatible with those that do have
                    // assigned parameters. For example, an empty Array that has
                    // not yet been mutated (and thus its parameter is
                    // unassigned) can be safely passed to something that
                    // expects e.g. `Array[Int]`.
                    return true;
                }

                our_class.type_parameters.values().clone().into_iter().all(
                    |param| {
                        our_args
                            .get(param)
                            .zip(their_args.get(param))
                            .map(|(ours, theirs)| {
                                ours.type_check(db, theirs, context, subtyping)
                            })
                            .unwrap_or(false)
                    },
                )
            }
            TypeId::TraitInstance(ins) => {
                self.type_check_with_trait_instance(db, ins, context, subtyping)
            }
            TypeId::TypeParameter(id) => {
                id.all_requirements_met(db, |db, req| {
                    self.type_check_with_trait_instance(
                        db, req, context, subtyping,
                    )
                })
            }
            _ => false,
        }
    }

    fn implements_trait_id(self, db: &Database, trait_id: TraitId) -> bool {
        self.instance_of.trait_implementation(db, trait_id).is_some()
    }

    fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        self.instance_of.named_type(db, name)
    }

    fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        if !self.instance_of.get(db).is_generic() {
            return self;
        }

        let new_args = TypeArguments::rigid(db, self.type_arguments, bounds);

        ClassInstance::generic(db, self.instance_of, new_args)
    }

    fn inferred(
        self,
        db: &mut Database,
        context: &mut TypeContext,
        immutable: bool,
    ) -> Self {
        if !self.instance_of.is_generic(db) {
            return self;
        }

        let mut new_args = TypeArguments::new();

        for (param, val) in self.type_arguments(db).pairs() {
            new_args.assign(param, val.inferred(db, context, immutable));
        }

        Self::generic(db, self.instance_of, new_args)
    }
}

impl FormatType for ClassInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            if ins_of.kind != ClassKind::Tuple {
                buffer.write(&ins_of.name);
            }

            if ins_of.type_parameters.len() > 0 {
                let (open, close) = if ins_of.kind == ClassKind::Tuple {
                    ("(", ")")
                } else {
                    ("[", "]")
                };

                let params = ins_of.type_parameters.values();
                let args = self.type_arguments(buffer.db);

                buffer.write(open);
                buffer.type_arguments(params, args);
                buffer.write(close);
            }
        });
    }
}

/// A collection of arguments.
#[derive(Clone)]
struct Arguments {
    mapping: IndexMap<String, Argument>,
}

impl Arguments {
    fn new() -> Self {
        Self { mapping: IndexMap::new() }
    }

    fn new_argument(
        &mut self,
        name: String,
        value_type: TypeRef,
        variable: VariableId,
    ) {
        let index = self.mapping.len();
        let arg = Argument { index, name: name.clone(), value_type, variable };

        self.mapping.insert(name, arg);
    }

    fn get(&self, name: &str) -> Option<&Argument> {
        self.mapping.get(name)
    }

    fn iter(&self) -> impl Iterator<Item = &Argument> {
        self.mapping.values().iter()
    }

    fn len(&self) -> usize {
        self.mapping.len()
    }

    fn type_check(
        &self,
        db: &mut Database,
        with: &Arguments,
        context: &mut TypeContext,
        same_name: bool,
    ) -> bool {
        if self.len() != with.len() {
            return false;
        }

        for (ours, theirs) in
            self.mapping.values().iter().zip(with.mapping.values().iter())
        {
            if same_name && ours.name != theirs.name {
                return false;
            }

            if !ours.value_type.type_check(
                db,
                theirs.value_type,
                context,
                false,
            ) {
                return false;
            }
        }

        true
    }
}

/// An argument defined in a method or closure.
#[derive(Clone)]
pub struct Argument {
    pub index: usize,
    pub name: String,
    pub value_type: TypeRef,
    pub variable: VariableId,
}

/// A block of code, such as a closure or method.
pub trait Block {
    fn new_argument(
        &self,
        db: &mut Database,
        name: String,
        variable_type: TypeRef,
        argument_type: TypeRef,
    ) -> VariableId;
    fn throw_type(&self, db: &Database) -> TypeRef;
    fn set_throw_type(&self, db: &mut Database, typ: TypeRef);
    fn return_type(&self, db: &Database) -> TypeRef;
    fn set_return_type(&self, db: &mut Database, typ: TypeRef);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// A public symbol can be used by any other module.
    Public,

    /// A symbol only available to the module in which it's defined.
    Private,

    /// A symbol only available inside the type that defined it.
    TypePrivate,
}

impl Visibility {
    pub fn public(public: bool) -> Visibility {
        if public {
            Self::Public
        } else {
            Self::Private
        }
    }

    pub fn is_private(self) -> bool {
        self != Self::Public
    }
}

#[derive(Copy, Clone)]
pub enum BuiltinFunctionKind {
    Function(BIF),
    Instruction(Opcode),
    Macro(CompilerMacro),
}

/// A function built into the compiler or VM.
pub struct BuiltinFunction {
    kind: BuiltinFunctionKind,
    return_type: TypeRef,
    throw_type: TypeRef,
}

impl BuiltinFunction {
    pub fn alloc(
        db: &mut Database,
        kind: BuiltinFunctionKind,
        name: &str,
        return_type: TypeRef,
        throw_type: TypeRef,
    ) -> BuiltinFunctionId {
        let func = Self { kind, return_type, throw_type };

        Self::add(db, name.to_string(), func)
    }

    fn add(db: &mut Database, name: String, func: Self) -> BuiltinFunctionId {
        let id = db.builtin_functions.len();

        db.builtin_functions.insert(name, func);
        BuiltinFunctionId(id)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BuiltinFunctionId(usize);

impl BuiltinFunctionId {
    pub fn kind(self, db: &Database) -> BuiltinFunctionKind {
        self.get(db).kind
    }

    pub fn return_type(self, db: &Database) -> TypeRef {
        self.get(db).return_type
    }

    pub fn throw_type(self, db: &Database) -> TypeRef {
        self.get(db).throw_type
    }

    fn get(self, db: &Database) -> &BuiltinFunction {
        &db.builtin_functions[self.0]
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MethodKind {
    /// An immutable asynchronous method.
    Async,

    /// A mutable asynchronous method.
    AsyncMutable,

    /// A static method.
    Static,

    /// A regular immutable instance method.
    Instance,

    /// An instance method that takes ownership of its receiver.
    Moving,

    /// A mutable instance method.
    Mutable,

    /// The method is a destructor.
    Destructor,
}

#[derive(Copy, Clone)]
pub enum MethodSource {
    /// The method is directly defined for a type.
    Direct,

    /// The method is defined using a regular trait implementation.
    Implementation(TraitInstance),

    /// The method is defined using a bounded trait implementation.
    BoundedImplementation(TraitInstance),
}

impl MethodSource {
    pub fn implementation(bounded: bool, instance: TraitInstance) -> Self {
        if bounded {
            Self::BoundedImplementation(instance)
        } else {
            Self::Implementation(instance)
        }
    }
}

pub enum MethodLookup {
    /// The method lookup is valid.
    Ok(MethodId),

    /// The method exists, but it's private and unavailable to the caller.
    Private,

    /// The method exists, but it's an instance method and the receiver is not
    /// an instance.
    InstanceOnStatic,

    /// The method exists, but it's a static method and the receiver is an
    /// instance.
    StaticOnInstance,

    /// The method doesn't exist.
    None,
}

/// A static or instance method.
#[derive(Clone)]
pub struct Method {
    module: ModuleId,
    name: String,
    kind: MethodKind,
    visibility: Visibility,
    type_parameters: IndexMap<String, TypeParameterId>,
    arguments: Arguments,
    throw_type: TypeRef,
    return_type: TypeRef,
    source: MethodSource,
    main: bool,

    /// The type of the receiver of the method, aka the type of `self` (not
    /// `Self`).
    receiver: TypeRef,

    /// The type to use for `Self` in this method.
    ///
    /// This differs from the receiver type. For example, for a static method
    /// the receiver type is the class, while the type of `Self` is an instance
    /// of the class.
    self_type: Option<TypeId>,

    /// The fields this method has access to, along with their types.
    field_types: HashMap<String, (FieldId, TypeRef)>,
}

impl Method {
    pub fn alloc(
        db: &mut Database,
        module: ModuleId,
        name: String,
        visibility: Visibility,
        kind: MethodKind,
    ) -> MethodId {
        let id = db.methods.len();
        let method = Method::new(module, name, visibility, kind);

        db.methods.push(method);
        MethodId(id)
    }

    fn new(
        module: ModuleId,
        name: String,
        visibility: Visibility,
        kind: MethodKind,
    ) -> Self {
        Self {
            module,
            name,
            kind,
            visibility,
            type_parameters: IndexMap::new(),
            arguments: Arguments::new(),
            throw_type: TypeRef::Never,
            return_type: TypeRef::Unknown,
            source: MethodSource::Direct,
            receiver: TypeRef::Unknown,
            self_type: None,
            field_types: HashMap::new(),
            main: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MethodId(usize);

impl MethodId {
    pub fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        self.get(db)
            .type_parameters
            .get(name)
            .map(|&id| Symbol::TypeParameter(id))
    }

    pub fn new_type_parameter(
        self,
        db: &mut Database,
        name: String,
    ) -> TypeParameterId {
        let param = TypeParameter::alloc(db, name.clone());

        self.get_mut(db).type_parameters.insert(name, param);
        param
    }

    pub fn set_receiver(self, db: &mut Database, receiver: TypeRef) {
        self.get_mut(db).receiver = receiver;
    }

    pub fn set_self_type(self, db: &mut Database, typ: TypeId) {
        self.get_mut(db).self_type = Some(typ);
    }

    pub fn receiver(self, db: &Database) -> TypeRef {
        self.get(db).receiver
    }

    pub fn self_type(self, db: &Database) -> TypeId {
        self.get(db).self_type.expect(
            "The method's Self type must be set before it can be obtained",
        )
    }

    pub fn source(self, db: &Database) -> MethodSource {
        self.get(db).source
    }

    pub fn set_source(self, db: &mut Database, source: MethodSource) {
        self.get_mut(db).source = source;
    }

    pub fn type_check(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db);
        let theirs = with.get(db);

        if ours.kind != theirs.kind {
            return false;
        }

        if ours.visibility != theirs.visibility {
            return false;
        }

        if ours.name != theirs.name {
            return false;
        }

        // These checks are performed in separate methods so we can avoid
        // borrowing conflicts of the type database.
        self.type_check_type_parameters(db, with, context)
            && self.type_check_arguments(db, with, context)
            && self.type_check_throw_type(db, with, context)
            && self.type_check_return_type(db, with, context)
    }

    fn type_check_type_parameters(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db);
        let theirs = with.get(db);

        if ours.type_parameters.len() != theirs.type_parameters.len() {
            return false;
        }

        ours.type_parameters
            .values()
            .clone()
            .into_iter()
            .zip(theirs.type_parameters.values().clone().into_iter())
            .all(|(ours, theirs)| {
                ours.type_check_with_type_parameter(db, theirs, context, false)
            })
    }

    fn type_check_arguments(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db).arguments.clone();
        let theirs = with.get(db).arguments.clone();

        ours.type_check(db, &theirs, context, true)
    }

    fn type_check_return_type(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db).return_type;
        let theirs = with.get(db).return_type;

        ours.type_check(db, theirs, context, true)
    }

    fn type_check_throw_type(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db).throw_type;
        let theirs = with.get(db).throw_type;

        ours.type_check(db, theirs, context, true)
    }

    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn is_private(self, db: &Database) -> bool {
        !self.is_public(db)
    }

    pub fn is_public(self, db: &Database) -> bool {
        self.get(db).visibility == Visibility::Public
    }

    pub fn is_mutable(self, db: &Database) -> bool {
        matches!(
            self.get(db).kind,
            MethodKind::Mutable | MethodKind::AsyncMutable
        )
    }

    pub fn is_immutable(self, db: &Database) -> bool {
        matches!(
            self.get(db).kind,
            MethodKind::Async | MethodKind::Static | MethodKind::Instance
        )
    }

    pub fn is_async(self, db: &Database) -> bool {
        matches!(
            self.get(db).kind,
            MethodKind::Async | MethodKind::AsyncMutable
        )
    }

    pub fn is_moving(self, db: &Database) -> bool {
        matches!(self.get(db).kind, MethodKind::Moving)
    }

    pub fn positional_argument_input_type(
        self,
        db: &Database,
        index: usize,
    ) -> Option<TypeRef> {
        self.get(db).arguments.mapping.get_index(index).map(|a| a.value_type)
    }

    pub fn arguments(self, db: &Database) -> Vec<Argument> {
        self.get(db).arguments.mapping.values().clone()
    }

    pub fn named_argument(
        self,
        db: &Database,
        name: &str,
    ) -> Option<(usize, TypeRef)> {
        self.get(db).arguments.get(name).map(|a| (a.index, a.value_type))
    }

    pub fn number_of_arguments(self, db: &Database) -> usize {
        self.get(db).arguments.len()
    }

    pub fn copy_method(self, db: &mut Database) -> MethodId {
        let copy = self.get(db).clone();
        let id = db.methods.len();

        db.methods.push(copy);
        MethodId(id)
    }

    pub fn mark_as_destructor(self, db: &mut Database) {
        self.get_mut(db).kind = MethodKind::Destructor;
    }

    pub fn kind(self, db: &Database) -> MethodKind {
        self.get(db).kind
    }

    pub fn module(self, db: &Database) -> ModuleId {
        self.get(db).module
    }

    pub fn ignore_return_value(self, db: &Database) -> bool {
        self.get(db).return_type == TypeRef::nil()
    }

    pub fn set_field_type(
        self,
        db: &mut Database,
        name: String,
        id: FieldId,
        value_type: TypeRef,
    ) {
        self.get_mut(db).field_types.insert(name, (id, value_type));
    }

    pub fn field_id_and_type(
        self,
        db: &Database,
        name: &str,
    ) -> Option<(FieldId, TypeRef)> {
        self.get(db).field_types.get(name).cloned()
    }

    pub fn fields(self, db: &Database) -> Vec<(FieldId, TypeRef)> {
        self.get(db).field_types.values().cloned().collect()
    }

    pub fn add_argument(&self, db: &mut Database, argument: Argument) {
        self.get_mut(db).arguments.new_argument(
            argument.name.clone(),
            argument.value_type,
            argument.variable,
        );
    }

    pub fn set_main(&self, db: &mut Database) {
        self.get_mut(db).main = true;
    }

    pub fn is_main(&self, db: &Database) -> bool {
        self.get(db).main
    }

    fn get(self, db: &Database) -> &Method {
        &db.methods[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut Method {
        &mut db.methods[self.0]
    }
}

impl Block for MethodId {
    fn new_argument(
        &self,
        db: &mut Database,
        name: String,
        variable_type: TypeRef,
        argument_type: TypeRef,
    ) -> VariableId {
        let var = Variable::alloc(db, name.clone(), variable_type, false);

        self.get_mut(db).arguments.new_argument(name, argument_type, var);
        var
    }

    fn set_throw_type(&self, db: &mut Database, typ: TypeRef) {
        self.get_mut(db).throw_type = typ;
    }

    fn set_return_type(&self, db: &mut Database, typ: TypeRef) {
        self.get_mut(db).return_type = typ;
    }

    fn throw_type(&self, db: &Database) -> TypeRef {
        self.get(db).throw_type
    }

    fn return_type(&self, db: &Database) -> TypeRef {
        self.get(db).return_type
    }
}

impl FormatType for MethodId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        let block = self.get(buffer.db);

        buffer.write("fn ");

        if block.visibility == Visibility::Public {
            buffer.write("pub ");
        }

        match block.kind {
            MethodKind::Async => buffer.write("async "),
            MethodKind::AsyncMutable => buffer.write("async mut "),
            MethodKind::Static => buffer.write("static "),
            MethodKind::Moving => buffer.write("move "),
            MethodKind::Mutable | MethodKind::Destructor => {
                buffer.write("mut ")
            }
            _ => {}
        }

        buffer.write(&block.name);

        if block.type_parameters.len() > 0 {
            buffer.write(" [");

            for (index, param) in
                block.type_parameters.values().iter().enumerate()
            {
                if index > 0 {
                    buffer.write(", ");
                }

                param.format_type(buffer);
            }

            buffer.write("]");
        }

        buffer.arguments(&block.arguments, true);
        buffer.throw_type(block.throw_type);
        buffer.return_type(block.return_type);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Receiver {
    /// The receiver is explicit (e.g. `foo.bar()`)
    Explicit,

    /// The receiver is implicitly `self` (e.g. `bar()` and there's an instance
    /// method with that name).
    Implicit,

    /// The receiver is implicit, and the method resolved to a module method.
    Module(ModuleId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallInfo {
    pub id: MethodId,
    pub receiver: Receiver,
    pub returns: TypeRef,
    pub throws: TypeRef,
    pub dynamic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureCallInfo {
    pub id: ClosureId,
    pub expected_arguments: Vec<TypeRef>,
    pub returns: TypeRef,
    pub throws: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltinCallInfo {
    pub id: BuiltinFunctionId,
    pub returns: TypeRef,
    pub throws: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub id: FieldId,
    pub variable_type: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallKind {
    Unknown,
    Call(CallInfo),
    ClosureCall(ClosureCallInfo),
    GetField(FieldInfo),
    SetField(FieldInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentifierKind {
    Unknown,
    Variable(VariableId),
    Module(ModuleId),
    Method(CallInfo),
    Field(FieldInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstantKind {
    Unknown,
    Constant(ConstantId),
    Class(ClassId),
    Method(CallInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstantPatternKind {
    Unknown,
    Variant(VariantId),
    String(ConstantId),
    Int(ConstantId),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Symbol {
    Class(ClassId),
    Trait(TraitId),
    Module(ModuleId),
    TypeParameter(TypeParameterId),
    Constant(ConstantId),
    Method(MethodId),
}

impl Symbol {
    pub fn is_public(self, db: &Database) -> bool {
        match self {
            Symbol::Method(id) => id.is_public(db),
            Symbol::Class(id) => id.is_public(db),
            Symbol::Trait(id) => id.is_public(db),
            Symbol::Constant(id) => id.is_public(db),
            _ => true,
        }
    }

    pub fn defined_in(self, db: &Database, module: ModuleId) -> bool {
        match self {
            Symbol::Method(id) => id.module(db) == module,
            Symbol::Class(id) => id.module(db) == module,
            Symbol::Trait(id) => id.module(db) == module,
            Symbol::Constant(id) => id.module(db) == module,
            _ => false,
        }
    }

    pub fn is_private(self, db: &Database) -> bool {
        !self.is_public(db)
    }
}

/// An Inko module.
pub struct Module {
    name: ModuleName,
    class: ClassId,
    file: PathBuf,
    constants: Vec<ConstantId>,
    symbols: HashMap<String, Symbol>,
}

impl Module {
    pub fn alloc(
        db: &mut Database,
        name: ModuleName,
        file: PathBuf,
    ) -> ModuleId {
        assert!(db.modules.len() <= u32::MAX as usize);

        let id = ModuleId(db.modules.len() as u32);
        let class_id = Class::alloc(
            db,
            name.to_string(),
            ClassKind::Regular,
            Visibility::Private,
            id,
        );

        db.module_mapping.insert(name.to_string(), id);
        db.modules.push(Module::new(name, class_id, file));
        id
    }

    fn new(name: ModuleName, class: ClassId, file: PathBuf) -> Module {
        Module {
            name,
            class,
            file,
            constants: Vec::new(),
            symbols: HashMap::default(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub fn name(self, db: &Database) -> &ModuleName {
        &self.get(db).name
    }

    pub fn file(self, db: &Database) -> PathBuf {
        self.get(db).file.clone()
    }

    pub fn symbol(self, db: &Database, name: &str) -> Option<Symbol> {
        self.get(db).symbols.get(name).cloned()
    }

    pub fn symbols(self, db: &Database) -> Vec<(String, Symbol)> {
        self.get(db)
            .symbols
            .iter()
            .map(|(name, value)| (name.clone(), *value))
            .collect()
    }

    pub fn symbol_exists(self, db: &Database, name: &str) -> bool {
        self.get(db).symbols.contains_key(name)
    }

    pub fn new_symbol(self, db: &mut Database, name: String, symbol: Symbol) {
        self.get_mut(db).symbols.insert(name, symbol);
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        self.get(db).class.method(db, name)
    }

    pub fn add_method(self, db: &mut Database, name: String, method: MethodId) {
        self.get(db).class.add_method(db, name, method);
    }

    pub fn is_std(self, db: &Database) -> bool {
        self.get(db).name.is_std()
    }

    pub fn class(self, db: &Database) -> ClassId {
        self.get(db).class
    }

    fn get(self, db: &Database) -> &Module {
        &db.modules[self.0 as usize]
    }

    fn get_mut(self, db: &mut Database) -> &mut Module {
        &mut db.modules[self.0 as usize]
    }
}

impl FormatType for ModuleId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name.to_string());
    }
}

/// A local variable.
pub struct Variable {
    /// The user-defined name of the variable.
    name: String,

    /// The type of the constant's value.
    value_type: TypeRef,

    /// A flat set to `true` if the variable can be assigned a new value.
    mutable: bool,
}

impl Variable {
    pub fn alloc(
        db: &mut Database,
        name: String,
        value_type: TypeRef,
        mutable: bool,
    ) -> VariableId {
        let id = VariableId(db.variables.len());

        db.variables.push(Self { name, value_type, mutable });
        id
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Ord, PartialOrd, Hash)]
pub struct VariableId(pub usize);

impl VariableId {
    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn value_type(self, db: &Database) -> TypeRef {
        self.get(db).value_type
    }

    pub fn is_mutable(self, db: &Database) -> bool {
        self.get(db).mutable
    }

    fn get(self, db: &Database) -> &Variable {
        &db.variables[self.0]
    }
}

/// A constant.
///
/// Unlike variables, constants can't be assigned new values. They are also
/// limited to values of a select few types.
pub struct Constant {
    /// The ID of the constant local to its module.
    id: u16,
    module: ModuleId,
    name: String,
    value_type: TypeRef,
    visibility: Visibility,
}

impl Constant {
    pub fn alloc(
        db: &mut Database,
        module: ModuleId,
        name: String,
        visibility: Visibility,
        value_type: TypeRef,
    ) -> ConstantId {
        let global_id = db.constants.len();
        let local_id = module.get(db).constants.len();

        assert!(local_id <= u16::MAX as usize);

        let constant = Constant {
            id: local_id as u16,
            module,
            name: name.clone(),
            value_type,
            visibility,
        };

        let const_id = ConstantId(global_id);

        module.get_mut(db).constants.push(const_id);
        module.new_symbol(db, name, Symbol::Constant(const_id));
        db.constants.push(constant);
        const_id
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ConstantId(pub usize);

impl ConstantId {
    pub fn module_local_id(self, db: &Database) -> u16 {
        self.get(db).id
    }

    pub fn name(self, db: &Database) -> &String {
        &self.get(db).name
    }

    pub fn module(self, db: &Database) -> ModuleId {
        self.get(db).module
    }

    pub fn set_value_type(self, db: &mut Database, value_type: TypeRef) {
        self.get_mut(db).value_type = value_type;
    }

    pub fn value_type(self, db: &Database) -> TypeRef {
        self.get(db).value_type
    }

    fn is_public(self, db: &Database) -> bool {
        self.get(db).visibility == Visibility::Public
    }

    fn get(self, db: &Database) -> &Constant {
        &db.constants[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut Constant {
        &mut db.constants[self.0]
    }
}

/// An anonymous function that can optionally capture outer variables.
///
/// Unlike methods, closures don't support type parameters. This makes it easier
/// to implement them, and generic closures aren't that useful to begin with. Of
/// course closures _can_ refer to type parameters defined in the surrounding
/// method or type.
#[derive(Clone)]
pub struct Closure {
    moving: bool,
    captured: HashSet<VariableId>,
    /// The type of `self` as captured by the closure.
    captured_self_type: Option<TypeRef>,
    arguments: Arguments,
    throw_type: TypeRef,
    return_type: TypeRef,
}

impl Closure {
    pub fn alloc(db: &mut Database, moving: bool) -> ClosureId {
        let closure = Closure::new(moving);

        Self::add(db, closure)
    }

    fn add(db: &mut Database, closure: Closure) -> ClosureId {
        let id = db.closures.len();

        db.closures.push(closure);
        ClosureId(id)
    }

    fn new(moving: bool) -> Self {
        Self {
            moving,
            captured_self_type: None,
            captured: HashSet::new(),
            arguments: Arguments::new(),
            throw_type: TypeRef::Never,
            return_type: TypeRef::Unknown,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClosureId(pub usize);

impl ClosureId {
    pub fn number_of_arguments(self, db: &Database) -> usize {
        self.get(db).arguments.len()
    }

    pub fn positional_argument_input_type(
        self,
        db: &Database,
        index: usize,
    ) -> Option<TypeRef> {
        self.get(db).arguments.mapping.get_index(index).map(|a| a.value_type)
    }

    pub fn new_anonymous_argument(
        self,
        db: &mut Database,
        value_type: TypeRef,
    ) {
        let lambda = self.get_mut(db);
        let name = lambda.arguments.len().to_string();

        // Anonymous arguments can never be used, so the variable ID is never
        // used. As such we just set it to ID 0 so we don't need to wrap it in
        // an `Option` type.
        let var = VariableId(0);

        lambda.arguments.new_argument(name, value_type, var);
    }

    pub fn is_moving(self, db: &Database) -> bool {
        self.get(db).moving
    }

    pub fn set_captured_self_type(
        self,
        db: &mut Database,
        value_type: TypeRef,
    ) {
        self.get_mut(db).captured_self_type = Some(value_type);
    }

    pub fn captured_self_type(self, db: &Database) -> Option<TypeRef> {
        self.get(db).captured_self_type
    }

    pub fn add_capture(self, db: &mut Database, variable: VariableId) {
        self.get_mut(db).captured.insert(variable);
    }

    pub fn captured(self, db: &Database) -> Vec<VariableId> {
        self.get(db).captured.iter().cloned().collect()
    }

    pub fn arguments(self, db: &Database) -> Vec<Argument> {
        self.get(db).arguments.mapping.values().clone()
    }

    pub fn can_infer_as_uni(self, db: &Database) -> bool {
        let closure = self.get(db);

        if !closure.captured.is_empty() {
            return false;
        }

        match closure.captured_self_type {
            Some(typ) if typ.is_permanent(db) => true,
            Some(_) => false,
            _ => true,
        }
    }

    fn type_check(
        self,
        db: &mut Database,
        with: TypeId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match with {
            TypeId::Closure(with) => {
                self.type_check_arguments(db, with, context)
                    && self.type_check_throw_type(db, with, context, subtyping)
                    && self.type_check_return_type(db, with, context, subtyping)
            }
            // Implementing traits for closures with a specific signature (e.g.
            // `impl ToString for do -> X`) isn't supported. Even if it was, it
            // probably wouldn't be useful. Implementing traits for all closures
            // isn't supported either, again because it isn't really useful.
            //
            // For this reason, we only consider a closures compatible with a
            // type parameter if the parameter has no requirements. This still
            // allows you to e.g. put a bunch of lambdas in an Array.
            TypeId::TypeParameter(id) => id.get(db).requirements.is_empty(),
            _ => false,
        }
    }

    fn type_check_arguments(
        self,
        db: &mut Database,
        with: ClosureId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db).arguments.clone();
        let theirs = with.get(db).arguments.clone();

        ours.type_check(db, &theirs, context, false)
    }

    fn type_check_return_type(
        self,
        db: &mut Database,
        with: ClosureId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        let ours = self.get(db).return_type;
        let theirs = with.get(db).return_type;

        ours.type_check(db, theirs, context, subtyping)
    }

    fn type_check_throw_type(
        self,
        db: &mut Database,
        with: ClosureId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        let ours = self.get(db).throw_type;
        let theirs = with.get(db).throw_type;

        ours.type_check(db, theirs, context, subtyping)
    }

    fn get(self, db: &Database) -> &Closure {
        &db.closures[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut Closure {
        &mut db.closures[self.0]
    }

    fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        let mut new_func = self.get(db).clone();

        new_func.throw_type = new_func.throw_type.as_rigid_type(db, bounds);
        new_func.return_type = new_func.return_type.as_rigid_type(db, bounds);

        Closure::add(db, new_func)
    }

    fn inferred(
        self,
        db: &mut Database,
        context: &mut TypeContext,
        immutable: bool,
    ) -> Self {
        let mut new_func = self.get(db).clone();

        for arg in new_func.arguments.mapping.values_mut() {
            arg.value_type = arg.value_type.inferred(db, context, immutable);
        }

        new_func.throw_type =
            new_func.throw_type.inferred(db, context, immutable);
        new_func.return_type =
            new_func.return_type.inferred(db, context, immutable);

        Closure::add(db, new_func)
    }
}

impl Block for ClosureId {
    fn new_argument(
        &self,
        db: &mut Database,
        name: String,
        variable_type: TypeRef,
        argument_type: TypeRef,
    ) -> VariableId {
        let var = Variable::alloc(db, name.clone(), variable_type, false);

        self.get_mut(db).arguments.new_argument(name, argument_type, var);
        var
    }

    fn set_throw_type(&self, db: &mut Database, typ: TypeRef) {
        self.get_mut(db).throw_type = typ;
    }

    fn set_return_type(&self, db: &mut Database, typ: TypeRef) {
        self.get_mut(db).return_type = typ;
    }

    fn throw_type(&self, db: &Database) -> TypeRef {
        self.get(db).throw_type
    }

    fn return_type(&self, db: &Database) -> TypeRef {
        self.get(db).return_type
    }
}

impl FormatType for ClosureId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let fun = self.get(buffer.db);

            if fun.moving {
                buffer.write("fn move");
            } else {
                buffer.write("fn");
            }

            buffer.arguments(&fun.arguments, false);
            buffer.throw_type(fun.throw_type);
            buffer.return_type(fun.return_type);
        });
    }
}

/// A reference to a type.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum TypeRef {
    /// An owned value subject to move semantics.
    Owned(TypeId),

    /// An owned value subject to move semantics, and doesn't allow aliasing.
    Uni(TypeId),

    /// An immutable reference to a type.
    Ref(TypeId),

    /// An immutable reference to a `uni T`.
    RefUni(TypeId),

    /// A mutable reference to a type.
    Mut(TypeId),

    /// A mutable reference to a `uni T`.
    MutUni(TypeId),

    /// A type of which the ownership should be inferred.
    ///
    /// This variant is only used with type parameters. We wrap a TypeId here so
    /// we can reuse various functions more easily, such as those used for
    /// type-checking; instead of having to special-case this variant.
    Infer(TypeId),

    /// A type that signals something never happens.
    ///
    /// When used as a return type, it means a method never returns. When used
    /// as an (explicit) throw type, it means the method never throws.
    Never,

    /// A value that could be anything _including_ non-managed objects.
    ///
    /// Values of these types _can_ be casted to other types, and they can be
    /// passed to other `Any` values. Beyond that, there's nothing you can do
    /// with them: they don't support method calls, pattern matching, etc.
    ///
    /// These types are used in a few places to allow interacting with internal
    /// types provided by the VM. Use of this type outside of the standard
    /// library isn't allowed.
    Any,

    /// A value that could be anything but shouldn't have its ownership
    /// transferred.
    RefAny,

    /// The `Self` type.
    OwnedSelf,

    /// The `uni Self` type.
    UniSelf,

    /// The `ref Self` type.
    RefSelf,

    /// The `mut Self` type.
    MutSelf,

    /// A value indicating a typing error.
    ///
    /// This type is produced whenever a type couldn't be produced, for example
    /// by calling a method on an undefined variable.
    Error,

    /// The type is not yet known.
    ///
    /// This is the default state for a type.
    Unknown,

    /// A placeholder for a yet-to-infer type.
    Placeholder(TypePlaceholderId),
}

impl TypeRef {
    fn mut_or_ref(id: TypeId, immutable: bool) -> TypeRef {
        if immutable {
            TypeRef::Ref(id)
        } else {
            TypeRef::Mut(id)
        }
    }

    fn mut_or_ref_uni(id: TypeId, immutable: bool) -> TypeRef {
        if immutable {
            TypeRef::RefUni(id)
        } else {
            TypeRef::MutUni(id)
        }
    }

    fn owned_or_ref(id: TypeId, immutable: bool) -> TypeRef {
        if immutable {
            TypeRef::Ref(id)
        } else {
            TypeRef::Owned(id)
        }
    }

    fn uni_or_ref(id: TypeId, immutable: bool) -> TypeRef {
        if immutable {
            TypeRef::RefUni(id)
        } else {
            TypeRef::Uni(id)
        }
    }

    pub fn nil() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(ClassId(
            NIL_ID,
        ))))
    }

    pub fn boolean() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(ClassId(
            BOOLEAN_ID,
        ))))
    }

    pub fn int() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(
            ClassInstance::new(ClassId::int()),
        ))
    }

    pub fn float() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(
            ClassId::float(),
        )))
    }

    pub fn string() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(
            ClassId::string(),
        )))
    }

    pub fn byte_array() -> TypeRef {
        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(
            ClassId::byte_array(),
        )))
    }

    pub fn array(db: &mut Database, value: TypeRef) -> TypeRef {
        let array_class = ClassId::array();
        let mut arguments = TypeArguments::new();
        let param = array_class.type_parameters(db)[0];

        arguments.assign(param, value);

        TypeRef::Owned(TypeId::ClassInstance(ClassInstance::generic(
            db,
            array_class,
            arguments,
        )))
    }

    pub fn module(id: ModuleId) -> TypeRef {
        TypeRef::Owned(TypeId::Module(id))
    }

    pub fn placeholder(db: &mut Database) -> TypeRef {
        TypeRef::Placeholder(TypePlaceholder::alloc(db))
    }

    pub fn type_id(
        self,
        db: &Database,
        self_type: TypeId,
    ) -> Result<TypeId, TypeRef> {
        match self {
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::RefUni(id)
            | TypeRef::MutUni(id)
            | TypeRef::Infer(id) => Ok(id),
            TypeRef::OwnedSelf
            | TypeRef::RefSelf
            | TypeRef::MutSelf
            | TypeRef::UniSelf => Ok(self_type),
            TypeRef::Placeholder(id) => {
                id.value(db).ok_or(self).and_then(|t| t.type_id(db, self_type))
            }
            _ => Err(self),
        }
    }

    pub fn closure_id(
        self,
        db: &Database,
        self_type: TypeId,
    ) -> Option<ClosureId> {
        if let Ok(TypeId::Closure(id)) = self.type_id(db, self_type) {
            Some(id)
        } else {
            None
        }
    }

    pub fn is_never(self, db: &Database) -> bool {
        match self {
            TypeRef::Never => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_never(db))
            }
            _ => false,
        }
    }

    pub fn is_any(self, db: &Database) -> bool {
        match self {
            TypeRef::Any | TypeRef::RefAny => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_any(db))
            }
            _ => false,
        }
    }

    pub fn is_ref_any(self, db: &Database) -> bool {
        match self {
            TypeRef::RefAny => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_ref_any(db))
            }
            _ => false,
        }
    }

    pub fn is_error(self, db: &Database) -> bool {
        match self {
            TypeRef::Error => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_error(db))
            }
            _ => false,
        }
    }

    pub fn is_present(self, db: &Database) -> bool {
        match self {
            TypeRef::Never => false,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_present(db))
            }
            _ => true,
        }
    }

    pub fn is_owned_or_uni(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(_)
            | TypeRef::Uni(_)
            | TypeRef::Infer(_)
            | TypeRef::UniSelf
            | TypeRef::OwnedSelf
            | TypeRef::Any => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_owned_or_uni(db))
            }
            _ => false,
        }
    }

    pub fn is_owned(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(_)
            | TypeRef::Infer(_)
            | TypeRef::OwnedSelf
            | TypeRef::Any => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_owned(db))
            }
            _ => false,
        }
    }

    pub fn is_type_parameter(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(TypeId::TypeParameter(_))
            | TypeRef::Uni(TypeId::TypeParameter(_))
            | TypeRef::Ref(TypeId::TypeParameter(_))
            | TypeRef::Mut(TypeId::TypeParameter(_))
            | TypeRef::Infer(TypeId::TypeParameter(_))
            | TypeRef::RefUni(TypeId::TypeParameter(_))
            | TypeRef::MutUni(TypeId::TypeParameter(_))
            | TypeRef::Owned(TypeId::RigidTypeParameter(_))
            | TypeRef::Uni(TypeId::RigidTypeParameter(_))
            | TypeRef::Ref(TypeId::RigidTypeParameter(_))
            | TypeRef::Mut(TypeId::RigidTypeParameter(_))
            | TypeRef::Infer(TypeId::RigidTypeParameter(_))
            | TypeRef::RefUni(TypeId::RigidTypeParameter(_))
            | TypeRef::MutUni(TypeId::RigidTypeParameter(_)) => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_type_parameter(db))
            }
            _ => false,
        }
    }

    pub fn is_trait_instance(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(TypeId::TraitInstance(_))
            | TypeRef::Uni(TypeId::TraitInstance(_))
            | TypeRef::Ref(TypeId::TraitInstance(_))
            | TypeRef::Mut(TypeId::TraitInstance(_))
            | TypeRef::RefUni(TypeId::TraitInstance(_))
            | TypeRef::MutUni(TypeId::TraitInstance(_)) => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_trait_instance(db))
            }
            _ => false,
        }
    }

    pub fn is_self_type(self, db: &Database) -> bool {
        match self {
            TypeRef::OwnedSelf | TypeRef::MutSelf | TypeRef::RefSelf => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_self_type(db))
            }
            _ => false,
        }
    }

    pub fn is_uni(self, db: &Database) -> bool {
        match self {
            TypeRef::Uni(_) | TypeRef::UniSelf => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_uni(db))
            }
            _ => false,
        }
    }

    pub fn require_sendable_arguments(self, db: &Database) -> bool {
        match self {
            TypeRef::Uni(_)
            | TypeRef::RefUni(_)
            | TypeRef::MutUni(_)
            | TypeRef::UniSelf => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.require_sendable_arguments(db))
            }
            _ => false,
        }
    }

    pub fn is_ref(self, db: &Database) -> bool {
        match self {
            TypeRef::Ref(_) | TypeRef::RefSelf => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_ref(db))
            }
            _ => false,
        }
    }

    pub fn is_mut(self, db: &Database) -> bool {
        match self {
            TypeRef::Mut(_) | TypeRef::MutSelf => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_ref(db))
            }
            _ => false,
        }
    }

    pub fn use_reference_counting(self, db: &Database) -> bool {
        match self {
            TypeRef::Ref(_)
            | TypeRef::RefSelf
            | TypeRef::Mut(_)
            | TypeRef::MutSelf
            | TypeRef::RefUni(_)
            | TypeRef::MutUni(_) => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.use_reference_counting(db))
            }
            _ => false,
        }
    }

    pub fn use_atomic_reference_counting(
        self,
        db: &Database,
        self_type: TypeId,
    ) -> bool {
        self.class_id(db, self_type)
            .map_or(false, |id| id.0 == STRING_ID || id.kind(db).is_async())
    }

    pub fn is_bool(self, db: &Database, self_type: TypeId) -> bool {
        self.is_instance_of(db, ClassId::boolean(), self_type)
    }

    pub fn is_string(self, db: &Database, self_type: TypeId) -> bool {
        self.is_instance_of(db, ClassId::string(), self_type)
    }

    pub fn is_nil(self, db: &Database, self_type: TypeId) -> bool {
        self.is_instance_of(db, ClassId::nil(), self_type)
    }

    pub fn allow_moving(self) -> bool {
        matches!(self, TypeRef::Owned(_) | TypeRef::Uni(_) | TypeRef::OwnedSelf)
    }

    pub fn allow_mutating(self) -> bool {
        matches!(
            self,
            TypeRef::Mut(_)
                | TypeRef::MutSelf
                | TypeRef::OwnedSelf
                | TypeRef::UniSelf
                | TypeRef::Owned(_)
                | TypeRef::Uni(_)
                | TypeRef::MutUni(_)
        )
    }

    pub fn allow_assignment(self, db: &Database) -> bool {
        match self {
            TypeRef::RefUni(_) | TypeRef::MutUni(_) => false,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(true, |v| v.allow_assignment(db))
            }
            _ => true,
        }
    }

    pub fn is_sendable(self, db: &Database) -> bool {
        if self.is_value_type(db) {
            return true;
        }

        match self {
            TypeRef::Uni(_)
            | TypeRef::UniSelf
            | TypeRef::Never
            | TypeRef::Error => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(true, |v| v.is_sendable(db))
            }
            _ => false,
        }
    }

    pub fn is_sendable_output(self, db: &Database) -> bool {
        if self.is_value_type(db) {
            return true;
        }

        match self {
            TypeRef::Uni(_)
            | TypeRef::UniSelf
            | TypeRef::Never
            | TypeRef::Any
            | TypeRef::Error => true,
            TypeRef::Owned(TypeId::ClassInstance(id)) => {
                let class = id.instance_of;

                if class.is_generic(db)
                    && !id
                        .type_arguments(db)
                        .mapping
                        .iter()
                        .all(|(_, v)| v.is_sendable_output(db))
                {
                    return false;
                }

                class
                    .fields(db)
                    .into_iter()
                    .all(|f| f.value_type(db).is_sendable_output(db))
            }
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(true, |v| v.is_sendable_output(db))
            }
            _ => false,
        }
    }

    pub fn is_async(self, db: &Database, self_type: TypeId) -> bool {
        self.class_id(db, self_type).map_or(false, |id| id.kind(db).is_async())
    }

    pub fn cast_according_to(self, other: Self, db: &Database) -> Self {
        if other.is_uni(db) && self.is_value_type(db) {
            self.as_uni(db)
        } else if other.is_ref(db) {
            self.as_ref(db)
        } else if other.is_mut(db) {
            self.as_mut(db)
        } else {
            self
        }
    }

    pub fn as_ref(self, db: &Database) -> Self {
        match self {
            TypeRef::Owned(id) | TypeRef::Infer(id) | TypeRef::Mut(id) => {
                TypeRef::Ref(id)
            }
            TypeRef::Uni(id) => TypeRef::RefUni(id),
            TypeRef::OwnedSelf => TypeRef::RefSelf,
            TypeRef::MutSelf => TypeRef::RefSelf,
            TypeRef::UniSelf => TypeRef::RefSelf,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_ref(db))
            }
            _ => self,
        }
    }

    pub fn as_mut(self, db: &Database) -> Self {
        match self {
            TypeRef::Owned(id) | TypeRef::Infer(id) => TypeRef::Mut(id),
            TypeRef::Uni(id) => TypeRef::MutUni(id),
            TypeRef::OwnedSelf | TypeRef::UniSelf => TypeRef::MutSelf,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_mut(db))
            }
            _ => self,
        }
    }

    pub fn as_ref_uni(self, db: &Database) -> Self {
        match self {
            TypeRef::Uni(id) => TypeRef::RefUni(id),
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_ref_uni(db))
            }
            _ => self,
        }
    }

    pub fn as_mut_uni(self, db: &Database) -> Self {
        match self {
            TypeRef::Uni(id) => TypeRef::MutUni(id),
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_mut_uni(db))
            }
            _ => self,
        }
    }

    pub fn as_uni(self, db: &Database) -> Self {
        match self {
            TypeRef::Owned(id) | TypeRef::Infer(id) | TypeRef::Uni(id) => {
                TypeRef::Uni(id)
            }
            TypeRef::OwnedSelf | TypeRef::UniSelf => TypeRef::UniSelf,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_uni(db))
            }
            _ => self,
        }
    }

    pub fn as_owned(self, db: &Database) -> Self {
        match self {
            TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::RefUni(id)
            | TypeRef::MutUni(id) => TypeRef::Owned(id),
            TypeRef::UniSelf | TypeRef::MutSelf | TypeRef::RefSelf => {
                TypeRef::OwnedSelf
            }
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(self, |v| v.as_owned(db))
            }
            _ => self,
        }
    }

    /// Replaces temporary types with their inferred types.
    pub fn inferred(
        self,
        db: &mut Database,
        context: &mut TypeContext,
        immutable: bool,
    ) -> TypeRef {
        if context.depth == MAX_TYPE_DEPTH {
            return TypeRef::Unknown;
        }

        context.depth += 1;

        let result = match self {
            TypeRef::OwnedSelf => TypeRef::owned_or_ref(
                self.infer_self_type_id(db, context),
                immutable,
            ),
            TypeRef::RefSelf => {
                TypeRef::Ref(self.infer_self_type_id(db, context))
            }
            TypeRef::MutSelf => TypeRef::mut_or_ref(
                self.infer_self_type_id(db, context),
                immutable,
            ),
            TypeRef::UniSelf => TypeRef::uni_or_ref(
                self.infer_self_type_id(db, context),
                immutable,
            ),
            // Owned and Infer variants are treated the same, because:
            //
            // 1. For non-type parameters, Infer(T) is the same as Owned(T)
            //    (simply because we never use Infer for anything but type
            //    parameters).
            // 2. For a type parameter, in both cases we can just use the
            //    assigned value if there is any, as said value determines the
            //    ownership. In case of an owned parameter non-owned values
            //    can't be assigned to it anyway.
            TypeRef::Owned(id) | TypeRef::Infer(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::owned_or_ref(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TraitInstance(tid) => TypeRef::owned_or_ref(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TypeParameter(pid) => {
                    let typ =
                        self.infer_type_parameter(pid, db, context, immutable);

                    if immutable {
                        typ.as_ref(db)
                    } else {
                        typ
                    }
                }
                TypeId::Closure(fid) => TypeRef::owned_or_ref(
                    TypeId::Closure(fid.inferred(db, context, immutable)),
                    immutable,
                ),
                _ => self,
            },
            TypeRef::Uni(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::uni_or_ref(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TraitInstance(tid) => TypeRef::uni_or_ref(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TypeParameter(pid) => {
                    let typ =
                        self.infer_type_parameter(pid, db, context, immutable);

                    if immutable {
                        typ.as_ref(db)
                    } else {
                        typ
                    }
                }
                TypeId::Closure(fid) => TypeRef::uni_or_ref(
                    TypeId::Closure(fid.inferred(db, context, immutable)),
                    immutable,
                ),
                _ => self,
            },
            TypeRef::Ref(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::Ref(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                ),
                TypeId::TraitInstance(tid) => TypeRef::Ref(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                ),
                TypeId::TypeParameter(pid) => self
                    .infer_type_parameter(pid, db, context, immutable)
                    .as_ref(db),
                TypeId::Closure(fid) => TypeRef::Ref(TypeId::Closure(
                    fid.inferred(db, context, immutable),
                )),
                _ => self,
            },
            TypeRef::RefUni(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::RefUni(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                ),
                TypeId::TraitInstance(tid) => TypeRef::RefUni(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                ),
                TypeId::TypeParameter(pid) => self
                    .infer_type_parameter(pid, db, context, immutable)
                    .as_ref_uni(db),
                TypeId::Closure(fid) => TypeRef::RefUni(TypeId::Closure(
                    fid.inferred(db, context, immutable),
                )),
                _ => self,
            },
            TypeRef::Mut(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::mut_or_ref(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TraitInstance(tid) => TypeRef::mut_or_ref(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TypeParameter(pid) => {
                    let typ =
                        self.infer_type_parameter(pid, db, context, immutable);

                    if immutable {
                        typ.as_ref(db)
                    } else {
                        typ.as_mut(db)
                    }
                }
                TypeId::Closure(fid) => TypeRef::mut_or_ref(
                    TypeId::Closure(fid.inferred(db, context, immutable)),
                    immutable,
                ),
                _ => self,
            },
            TypeRef::MutUni(id) => match id {
                TypeId::ClassInstance(cid) => TypeRef::mut_or_ref_uni(
                    TypeId::ClassInstance(cid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TraitInstance(tid) => TypeRef::mut_or_ref_uni(
                    TypeId::TraitInstance(tid.inferred(db, context, immutable)),
                    immutable,
                ),
                TypeId::TypeParameter(pid) => {
                    let typ =
                        self.infer_type_parameter(pid, db, context, immutable);

                    if immutable {
                        typ.as_ref_uni(db)
                    } else {
                        typ.as_mut_uni(db)
                    }
                }
                TypeId::Closure(fid) => TypeRef::mut_or_ref_uni(
                    TypeId::Closure(fid.inferred(db, context, immutable)),
                    immutable,
                ),
                _ => self,
            },
            TypeRef::Placeholder(id) => id
                .value(db)
                .map(|t| t.inferred(db, context, immutable))
                .unwrap_or_else(
                    || if immutable { self.as_ref(db) } else { self },
                ),
            _ => self,
        };

        context.depth -= 1;
        result
    }

    pub fn as_enum_instance(
        self,
        db: &Database,
        self_type: TypeId,
    ) -> Option<ClassInstance> {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
                if ins.instance_of.kind(db).is_enum() =>
            {
                Some(ins)
            }
            TypeRef::OwnedSelf
            | TypeRef::RefSelf
            | TypeRef::MutSelf
            | TypeRef::UniSelf => match self_type {
                TypeId::ClassInstance(ins)
                    if ins.instance_of.kind(db).is_enum() =>
                {
                    Some(ins)
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_type_parameter(self) -> Option<TypeParameterId> {
        match self {
            TypeRef::Owned(TypeId::TypeParameter(id))
            | TypeRef::Uni(TypeId::TypeParameter(id))
            | TypeRef::Ref(TypeId::TypeParameter(id))
            | TypeRef::Mut(TypeId::TypeParameter(id))
            | TypeRef::Infer(TypeId::TypeParameter(id))
            | TypeRef::Owned(TypeId::RigidTypeParameter(id))
            | TypeRef::Uni(TypeId::RigidTypeParameter(id))
            | TypeRef::Ref(TypeId::RigidTypeParameter(id))
            | TypeRef::Mut(TypeId::RigidTypeParameter(id))
            | TypeRef::RefUni(TypeId::RigidTypeParameter(id))
            | TypeRef::MutUni(TypeId::RigidTypeParameter(id))
            | TypeRef::Infer(TypeId::RigidTypeParameter(id)) => Some(id),
            _ => None,
        }
    }

    pub fn fields(self, db: &Database) -> Vec<FieldId> {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins)) => {
                ins.instance_of().fields(db)
            }
            TypeRef::Placeholder(id) => {
                id.value(db).map_or_else(Vec::new, |v| v.fields(db))
            }
            _ => Vec::new(),
        }
    }

    fn is_regular_type_parameter(self) -> bool {
        matches!(
            self,
            TypeRef::Owned(TypeId::TypeParameter(_))
                | TypeRef::Uni(TypeId::TypeParameter(_))
                | TypeRef::Ref(TypeId::TypeParameter(_))
                | TypeRef::Mut(TypeId::TypeParameter(_))
                | TypeRef::Infer(TypeId::TypeParameter(_))
                | TypeRef::RefUni(TypeId::TypeParameter(_))
                | TypeRef::MutUni(TypeId::TypeParameter(_))
        )
    }

    pub fn is_compatible_with_type_parameter(
        self,
        db: &mut Database,
        parameter: TypeParameterId,
        context: &mut TypeContext,
    ) -> bool {
        parameter
            .requirements(db)
            .into_iter()
            .all(|r| self.implements_trait_instance(db, r, context))
    }

    pub fn allow_cast_to(
        self,
        db: &mut Database,
        with: TypeRef,
        context: &mut TypeContext,
    ) -> bool {
        // Casting to/from Any is dangerous but necessary to make the standard
        // library work.
        if self == TypeRef::Any || with == TypeRef::Any {
            return true;
        }

        self.type_check_directly(db, with, context, true)
    }

    pub fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        match self {
            TypeRef::Owned(id) => TypeRef::Owned(id.as_rigid_type(db, bounds)),
            TypeRef::Uni(id) => TypeRef::Uni(id.as_rigid_type(db, bounds)),
            TypeRef::Ref(id) => TypeRef::Ref(id.as_rigid_type(db, bounds)),
            TypeRef::Mut(id) => TypeRef::Mut(id.as_rigid_type(db, bounds)),
            TypeRef::Infer(id) => TypeRef::Owned(id.as_rigid_type(db, bounds)),
            _ => self,
        }
    }

    pub fn implements_trait_instance(
        self,
        db: &mut Database,
        trait_type: TraitInstance,
        context: &mut TypeContext,
    ) -> bool {
        match self {
            TypeRef::Any => false,
            TypeRef::Error => true,
            TypeRef::Never => true,
            TypeRef::OwnedSelf
            | TypeRef::RefSelf
            | TypeRef::MutSelf
            | TypeRef::UniSelf => context
                .self_type
                .implements_trait_instance(db, trait_type, context),
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::Infer(id) => {
                id.implements_trait_instance(db, trait_type, context)
            }
            TypeRef::Placeholder(id) => id.value(db).map_or(true, |v| {
                v.implements_trait_instance(db, trait_type, context)
            }),
            _ => false,
        }
    }

    pub fn is_value_type(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
                if ins.instance_of.kind(db).is_async() =>
            {
                true
            }
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins)) => {
                matches!(
                    ins.instance_of.0,
                    INT_ID | FLOAT_ID | STRING_ID | BOOLEAN_ID | NIL_ID
                )
            }
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(true, |v| v.is_value_type(db))
            }
            _ => false,
        }
    }

    pub fn is_permanent(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins)) => {
                matches!(ins.instance_of.0, BOOLEAN_ID | NIL_ID)
            }
            TypeRef::Owned(TypeId::Module(_)) => true,
            TypeRef::Owned(TypeId::Class(_)) => true,
            TypeRef::Never => true,
            TypeRef::Any => true,
            TypeRef::RefAny => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(true, |v| v.is_permanent(db))
            }
            _ => false,
        }
    }

    pub fn is_inferred(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::RefUni(id)
            | TypeRef::MutUni(id)
            | TypeRef::Infer(id) => {
                let args = match id {
                    TypeId::ClassInstance(ins)
                        if ins.instance_of.is_generic(db) =>
                    {
                        ins.type_arguments(db)
                    }
                    TypeId::TraitInstance(ins)
                        if ins.instance_of.is_generic(db) =>
                    {
                        ins.type_arguments(db)
                    }
                    _ => return true,
                };

                args.mapping.values().all(|v| v.is_inferred(db))
            }
            TypeRef::Placeholder(id) => id.value(db).is_some(),
            _ => true,
        }
    }

    pub fn implements_trait_id(
        self,
        db: &Database,
        trait_id: TraitId,
        self_type: TypeId,
    ) -> bool {
        match self {
            TypeRef::Any => false,
            TypeRef::Error => false,
            TypeRef::Never => false,
            TypeRef::OwnedSelf
            | TypeRef::RefSelf
            | TypeRef::MutSelf
            | TypeRef::UniSelf => self_type.implements_trait_id(db, trait_id),
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::Infer(id) => id.implements_trait_id(db, trait_id),
            _ => false,
        }
    }

    pub fn class_id(self, db: &Database, self_type: TypeId) -> Option<ClassId> {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins)) => Some(ins.instance_of),
            TypeRef::OwnedSelf | TypeRef::RefSelf | TypeRef::MutSelf => {
                match self_type {
                    TypeId::ClassInstance(ins) => Some(ins.instance_of),
                    _ => None,
                }
            }
            TypeRef::Placeholder(p) => {
                p.value(db).and_then(|v| v.class_id(db, self_type))
            }
            _ => None,
        }
    }

    pub fn type_check(
        self,
        db: &mut Database,
        with: TypeRef,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        // We special-case type parameters on the right-hand side here, that way
        // we don't need to cover this case for all the various TypeRef variants
        // individually.
        match with {
            TypeRef::Owned(TypeId::TypeParameter(pid))
            | TypeRef::Uni(TypeId::TypeParameter(pid))
            | TypeRef::Infer(TypeId::TypeParameter(pid))
            | TypeRef::RefUni(TypeId::TypeParameter(pid))
            | TypeRef::MutUni(TypeId::TypeParameter(pid))
            | TypeRef::Mut(TypeId::TypeParameter(pid))
            | TypeRef::Ref(TypeId::TypeParameter(pid)) => self
                .type_check_with_type_parameter(
                    db, with, pid, context, subtyping,
                ),
            TypeRef::Placeholder(id) => {
                if let Some(assigned) = id.value(db) {
                    self.type_check_directly(db, assigned, context, subtyping)
                } else if let TypeRef::Placeholder(ours) = self {
                    // Assigning a placeholder to an unassigned placeholder
                    // isn't useful, and can break type inference when returning
                    // empty generic types in e.g. a closure (as this will
                    // compare them to a type placeholder).
                    //
                    // Instead, we track our placeholder in the one we're
                    // comparing with, ensuring our placeholder is also assigned
                    // when the one we're comparing with is assigned a value.
                    id.add_depending(db, ours);
                    true
                } else {
                    id.assign(db, self);
                    true
                }
            }
            _ => self.type_check_directly(db, with, context, subtyping),
        }
    }

    fn type_check_with_type_parameter(
        self,
        db: &mut Database,
        with: TypeRef,
        param: TypeParameterId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        if let Some(assigned) = context.type_arguments.get(param) {
            if let TypeRef::Placeholder(placeholder) = assigned {
                let mut rhs = with;
                let mut update = true;

                if let Some(val) = placeholder.value(db) {
                    rhs = val;

                    // A placeholder may be assigned to a regular type
                    // parameter (not a rigid one). In this case we want to
                    // update the placeholder value to `self`. An example of
                    // where this can happen is the following:
                    //
                    //     class Stack[V] { @values: Array[V] }
                    //     Stack { @values = [] }
                    //
                    // Here `[]` is of type `Array[P]` where P is a placeholder.
                    // When assigned to `@values`, we end up assigning V to P,
                    // and P to V.
                    //
                    // If the parameter is rigid we have to leave it as-is, as
                    // inferring the types further is unsafe.
                    update = matches!(
                        val,
                        TypeRef::Owned(TypeId::TypeParameter(_))
                            | TypeRef::Uni(TypeId::TypeParameter(_))
                            | TypeRef::Ref(TypeId::TypeParameter(_))
                            | TypeRef::Mut(TypeId::TypeParameter(_))
                            | TypeRef::Infer(TypeId::TypeParameter(_))
                    );
                }

                rhs = rhs.cast_according_to(with, db);

                let compat =
                    self.type_check_directly(db, rhs, context, subtyping);

                if compat && update {
                    placeholder.assign(db, self);
                }

                return compat;
            }

            return self.type_check_directly(
                db,
                assigned.cast_according_to(with, db),
                context,
                subtyping,
            );
        }

        if self.type_check_directly(db, with, context, subtyping) {
            context.type_arguments.assign(param, self);

            return true;
        }

        false
    }

    fn type_check_directly(
        self,
        db: &mut Database,
        with: TypeRef,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match self {
            TypeRef::Owned(our_id) => match with {
                TypeRef::Owned(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Any | TypeRef::RefAny | TypeRef::Error => true,
                TypeRef::OwnedSelf => {
                    our_id.type_check(db, context.self_type, context, subtyping)
                }
                _ => false,
            },
            TypeRef::Uni(our_id) => match with {
                TypeRef::Owned(their_id)
                | TypeRef::Infer(their_id)
                | TypeRef::Uni(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Any | TypeRef::RefAny | TypeRef::Error => true,
                TypeRef::UniSelf => {
                    our_id.type_check(db, context.self_type, context, subtyping)
                }
                _ => false,
            },
            TypeRef::RefUni(our_id) => match with {
                TypeRef::RefUni(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::MutUni(our_id) => match with {
                TypeRef::RefUni(their_id) | TypeRef::MutUni(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Ref(our_id) => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Error => true,
                TypeRef::RefSelf => {
                    our_id.type_check(db, context.self_type, context, subtyping)
                }
                _ => false,
            },
            TypeRef::Mut(our_id) => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Mut(their_id) => {
                    our_id.type_check(db, their_id, context, false)
                }
                TypeRef::Error => true,
                TypeRef::RefSelf => {
                    our_id.type_check(db, context.self_type, context, subtyping)
                }
                TypeRef::MutSelf => {
                    our_id.type_check(db, context.self_type, context, false)
                }
                _ => false,
            },
            TypeRef::Infer(our_id) => match with {
                TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, subtyping)
                }
                TypeRef::Error => true,
                _ => false,
            },
            // Since a Never can't actually be passed around, it's compatible
            // with everything else. This allows for code like this:
            //
            //     try foo else panic
            //
            // Where `panic` would return a `Never`.
            TypeRef::Never => true,
            TypeRef::OwnedSelf => match with {
                TypeRef::Owned(their_id) | TypeRef::Infer(their_id) => context
                    .self_type
                    .type_check(db, their_id, context, subtyping),
                TypeRef::Any
                | TypeRef::RefAny
                | TypeRef::Error
                | TypeRef::OwnedSelf => true,
                _ => false,
            },
            TypeRef::RefSelf => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => context
                    .self_type
                    .type_check(db, their_id, context, subtyping),
                TypeRef::Error | TypeRef::RefSelf => true,
                _ => false,
            },
            TypeRef::MutSelf => match with {
                TypeRef::Mut(their_id) | TypeRef::Infer(their_id) => {
                    context.self_type.type_check(db, their_id, context, false)
                }
                TypeRef::Error | TypeRef::MutSelf => true,
                _ => false,
            },
            TypeRef::UniSelf => match with {
                TypeRef::Owned(their_id)
                | TypeRef::Uni(their_id)
                | TypeRef::Infer(their_id) => {
                    context.self_type.type_check(db, their_id, context, false)
                }
                TypeRef::Any
                | TypeRef::RefAny
                | TypeRef::Error
                | TypeRef::UniSelf
                | TypeRef::OwnedSelf => true,
                _ => false,
            },
            // Type errors are compatible with all other types to prevent a
            // cascade of type errors.
            TypeRef::Error => true,
            TypeRef::Any => {
                matches!(with, TypeRef::Any | TypeRef::RefAny | TypeRef::Error)
            }
            TypeRef::RefAny => matches!(with, TypeRef::RefAny | TypeRef::Error),
            TypeRef::Placeholder(id) => {
                if let Some(assigned) = id.value(db) {
                    return assigned.type_check(db, with, context, subtyping);
                }

                if !with.is_regular_type_parameter() {
                    // This is best explained with an example. Consider the
                    // following code:
                    //
                    //     class Stack[X] {
                    //       @values: Array[X]
                    //
                    //       static fn new -> Self {
                    //         Self { @values = [] }
                    //       }
                    //     }
                    //
                    // When the array is created, it's type is `Array[?]` where
                    // `?` is a placeholder. When assigned to `Array[X]`, we end
                    // up comparing the placeholder to `X`. The type of `X` in
                    // this case is `Infer(X)`, because we don't know the
                    // ownership at runtime.
                    //
                    // The return type is expected to be a rigid type (i.e.
                    // literally `Stack[X]` and not e.g. `Stack[Int]`). This
                    // creates a problem: Infer() isn't compatible with a rigid
                    // type parameter, so the above code would produce a type
                    // error.
                    //
                    // In addition, it introduces a cycle of `X -> ? -> X`
                    // that's not needed.
                    //
                    // To prevent both from happening, we _don't_ assign to the
                    // placeholder if the assigned value is a regular type
                    // parameter. Regular type parameters can't occur outside of
                    // method signatures, as they are either turned into rigid
                    // parameters or replaced with placeholders.
                    id.assign(db, with);
                }

                true
            }
            _ => false,
        }
    }

    fn infer_self_type_id(
        self,
        db: &mut Database,
        context: &TypeContext,
    ) -> TypeId {
        // Self types always refer to instances of a type, so if
        // `context.self_type` is a class or trait, we need to turn it into an
        // instance.
        match context.self_type {
            TypeId::Class(id) => {
                let ins = if id.is_generic(db) {
                    let args = context
                        .type_arguments
                        .assigned_or_placeholders(db, id.type_parameters(db));

                    ClassInstance::generic(db, id, args)
                } else {
                    ClassInstance::new(id)
                };

                TypeId::ClassInstance(ins)
            }
            TypeId::Trait(id) => {
                let ins = if id.is_generic(db) {
                    let args = context
                        .type_arguments
                        .assigned_or_placeholders(db, id.type_parameters(db));

                    TraitInstance::generic(db, id, args)
                } else {
                    TraitInstance::new(id)
                };

                TypeId::TraitInstance(ins)
            }
            val => val,
        }
    }

    fn infer_type_parameter(
        self,
        type_parameter: TypeParameterId,
        db: &mut Database,
        context: &mut TypeContext,
        immutable: bool,
    ) -> TypeRef {
        if let Some(arg) = context.type_arguments.get(type_parameter) {
            // Given a case of `A -> placeholder -> A`, this prevents us from
            // recursing back into this code and eventually blowing up the
            // stack.
            if let TypeRef::Placeholder(id) = arg {
                if id.value(db).map_or(false, |v| v == self) {
                    return arg;
                }
            }

            if arg == self {
                return self;
            }

            return arg.inferred(db, context, immutable);
        }

        if let TypeId::TraitInstance(ins) = context.self_type {
            if let Some(arg) = ins
                .instance_of
                .get(db)
                .inherited_type_arguments
                .get(type_parameter)
            {
                return arg.inferred(db, context, immutable);
            }
        }

        TypeRef::placeholder(db)
    }

    fn format_self_type(self, buffer: &mut TypeFormatter) {
        if let Some(val) = buffer.self_type {
            val.format_type(buffer);
        } else {
            buffer.write("Self");
        }
    }

    fn is_instance_of(
        self,
        db: &Database,
        id: ClassId,
        self_type: TypeId,
    ) -> bool {
        self.class_id(db, self_type) == Some(id)
    }
}

impl FormatType for TypeRef {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeRef::Owned(id) | TypeRef::Infer(id) => id.format_type(buffer),
            TypeRef::Uni(id) => {
                buffer.write_ownership("uni ");
                id.format_type(buffer);
            }
            TypeRef::RefUni(id) => {
                buffer.write_ownership("ref uni ");
                id.format_type(buffer);
            }
            TypeRef::MutUni(id) => {
                buffer.write_ownership("mut uni ");
                id.format_type(buffer);
            }
            TypeRef::Ref(id) => {
                buffer.write_ownership("ref ");
                id.format_type(buffer);
            }
            TypeRef::Mut(id) => {
                buffer.write_ownership("mut ");
                id.format_type(buffer);
            }
            TypeRef::Never => buffer.write("Never"),
            TypeRef::Any => buffer.write("Any"),
            TypeRef::RefAny => buffer.write("ref Any"),
            TypeRef::OwnedSelf => {
                self.format_self_type(buffer);
            }
            TypeRef::RefSelf => {
                buffer.write_ownership("ref ");
                self.format_self_type(buffer);
            }
            TypeRef::MutSelf => {
                buffer.write_ownership("mut ");
                self.format_self_type(buffer);
            }
            TypeRef::UniSelf => {
                buffer.write_ownership("uni ");
                self.format_self_type(buffer);
            }
            TypeRef::Error => buffer.write("<error>"),
            TypeRef::Unknown => buffer.write("<unknown>"),
            TypeRef::Placeholder(id) => id.format_type(buffer),
        };
    }
}

/// An ID pointing to a type.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum TypeId {
    Class(ClassId),
    Trait(TraitId),
    Module(ModuleId),
    ClassInstance(ClassInstance),
    TraitInstance(TraitInstance),
    TypeParameter(TypeParameterId),
    RigidTypeParameter(TypeParameterId),
    Closure(ClosureId),
}

impl TypeId {
    pub fn named_type(self, db: &Database, name: &str) -> Option<Symbol> {
        match self {
            TypeId::Module(id) => id.symbol(db, name),
            TypeId::Trait(id) => id.named_type(db, name),
            TypeId::Class(id) => id.named_type(db, name),
            TypeId::ClassInstance(id) => id.named_type(db, name),
            TypeId::TraitInstance(id) => id.named_type(db, name),
            _ => None,
        }
    }

    pub fn lookup_method(
        self,
        db: &Database,
        name: &str,
        module: ModuleId,
        allow_type_private: bool,
    ) -> MethodLookup {
        if let Some(id) = self.method(db, name) {
            let kind = id.kind(db);
            let is_ins = !matches!(self, TypeId::Class(_) | TypeId::Trait(_));

            if is_ins && kind == MethodKind::Static {
                MethodLookup::StaticOnInstance
            } else if !is_ins && kind != MethodKind::Static {
                MethodLookup::InstanceOnStatic
            } else if self.can_call(db, id, module, allow_type_private) {
                MethodLookup::Ok(id)
            } else {
                MethodLookup::Private
            }
        } else {
            MethodLookup::None
        }
    }

    pub fn method(self, db: &Database, name: &str) -> Option<MethodId> {
        match self {
            TypeId::Class(id) => id.method(db, name),
            TypeId::Trait(id) => id.method(db, name),
            TypeId::Module(id) => id.method(db, name),
            TypeId::ClassInstance(id) => id.method(db, name),
            TypeId::TraitInstance(id) => id.method(db, name),
            TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id) => {
                id.method(db, name)
            }
            TypeId::Closure(_) => None,
        }
    }

    pub fn implements_trait_instance(
        self,
        db: &mut Database,
        trait_type: TraitInstance,
        context: &mut TypeContext,
    ) -> bool {
        match self {
            TypeId::ClassInstance(id) => {
                id.type_check_with_trait_instance(db, trait_type, context, true)
            }
            TypeId::TraitInstance(id) => {
                id.implements_trait_instance(db, trait_type, context)
            }
            TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id) => {
                id.type_check_with_trait_instance(db, trait_type, context, true)
            }
            _ => false,
        }
    }

    pub fn use_dynamic_dispatch(self) -> bool {
        matches!(
            self,
            TypeId::TraitInstance(_)
                | TypeId::TypeParameter(_)
                | TypeId::RigidTypeParameter(_)
        )
    }

    pub fn has_destructor(self, db: &Database) -> bool {
        if let TypeId::ClassInstance(id) = self {
            id.instance_of().has_destructor(db)
        } else {
            false
        }
    }

    fn implements_trait_id(self, db: &Database, trait_id: TraitId) -> bool {
        match self {
            TypeId::ClassInstance(id) => id.implements_trait_id(db, trait_id),
            TypeId::TraitInstance(id) => id.implements_trait_id(db, trait_id),
            TypeId::TypeParameter(id) => id
                .requirements(db)
                .iter()
                .any(|req| req.implements_trait_id(db, trait_id)),
            _ => false,
        }
    }

    fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        match self {
            TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_) => self,
            TypeId::ClassInstance(ins) => {
                TypeId::ClassInstance(ins.as_rigid_type(db, bounds))
            }
            TypeId::TraitInstance(ins) => {
                TypeId::TraitInstance(ins.as_rigid_type(db, bounds))
            }
            TypeId::TypeParameter(ins) => ins.as_rigid_type(bounds),
            TypeId::RigidTypeParameter(_) => self,
            TypeId::Closure(ins) => {
                TypeId::Closure(ins.as_rigid_type(db, bounds))
            }
        }
    }

    fn can_call(
        self,
        db: &Database,
        method: MethodId,
        module: ModuleId,
        allow_type_private: bool,
    ) -> bool {
        let m = method.get(db);

        if m.kind == MethodKind::Destructor {
            return false;
        }

        match m.visibility {
            Visibility::Public => true,
            Visibility::Private => m.module == module,
            Visibility::TypePrivate => allow_type_private,
        }
    }

    fn type_check(
        self,
        db: &mut Database,
        with: TypeId,
        context: &mut TypeContext,
        subtyping: bool,
    ) -> bool {
        match self {
            TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_) => {
                self == with
            }
            TypeId::ClassInstance(ins) => {
                ins.type_check(db, with, context, subtyping)
            }
            TypeId::TraitInstance(ins) => {
                ins.type_check(db, with, context, subtyping)
            }
            TypeId::TypeParameter(ins) => {
                ins.type_check(db, with, context, subtyping)
            }
            TypeId::RigidTypeParameter(our_ins) => match with {
                TypeId::RigidTypeParameter(their_ins) => our_ins == their_ins,
                _ => our_ins.type_check(db, with, context, subtyping),
            },
            TypeId::Closure(ins) => {
                ins.type_check(db, with, context, subtyping)
            }
        }
    }
}

impl FormatType for TypeId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeId::Class(id) => id.format_type(buffer),
            TypeId::Trait(id) => id.format_type(buffer),
            TypeId::Module(id) => id.format_type(buffer),
            TypeId::ClassInstance(ins) => ins.format_type(buffer),
            TypeId::TraitInstance(id) => id.format_type(buffer),
            TypeId::TypeParameter(id) => id.format_type(buffer),
            TypeId::RigidTypeParameter(id) => {
                id.format_type_without_argument(buffer);
            }
            TypeId::Closure(id) => id.format_type(buffer),
        }
    }
}

/// A database of all Inko types.
pub struct Database {
    modules: Vec<Module>,
    module_mapping: HashMap<String, ModuleId>,
    traits: Vec<Trait>,
    classes: Vec<Class>,
    type_parameters: Vec<TypeParameter>,
    type_arguments: Vec<TypeArguments>,
    methods: Vec<Method>,
    fields: Vec<Field>,
    closures: Vec<Closure>,
    variables: Vec<Variable>,
    constants: Vec<Constant>,
    builtin_functions: IndexMap<String, BuiltinFunction>,
    type_placeholders: Vec<TypePlaceholder>,
    variants: Vec<Variant>,

    /// The module that acts as the entry point of the program.
    ///
    /// For executables this will be set based on the file that is built/run.
    /// When just type-checking a project, this may be left as a None.
    main_module: Option<ModuleName>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            module_mapping: HashMap::new(),
            traits: Vec::new(),
            classes: vec![
                Class::regular(INT_NAME.to_string()),
                Class::regular(FLOAT_NAME.to_string()),
                Class::regular(STRING_NAME.to_string()),
                Class::regular(ARRAY_NAME.to_string()),
                Class::regular(BOOLEAN_NAME.to_string()),
                Class::regular(NIL_NAME.to_string()),
                Class::regular(BYTE_ARRAY_NAME.to_string()),
                Class::regular(FUTURE_NAME.to_string()),
                Class::tuple(TUPLE1_NAME.to_string()),
                Class::tuple(TUPLE2_NAME.to_string()),
                Class::tuple(TUPLE3_NAME.to_string()),
                Class::tuple(TUPLE4_NAME.to_string()),
                Class::tuple(TUPLE5_NAME.to_string()),
                Class::tuple(TUPLE6_NAME.to_string()),
                Class::tuple(TUPLE7_NAME.to_string()),
                Class::tuple(TUPLE8_NAME.to_string()),
            ],
            type_parameters: Vec::new(),
            type_arguments: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            closures: Vec::new(),
            variables: Vec::new(),
            constants: Vec::new(),
            builtin_functions: IndexMap::new(),
            type_placeholders: Vec::new(),
            variants: Vec::new(),
            main_module: None,
        }
    }

    pub fn builtin_class(&self, name: &str) -> Option<ClassId> {
        match name {
            INT_NAME => Some(ClassId::int()),
            FLOAT_NAME => Some(ClassId::float()),
            STRING_NAME => Some(ClassId::string()),
            ARRAY_NAME => Some(ClassId(ARRAY_ID)),
            BOOLEAN_NAME => Some(ClassId(BOOLEAN_ID)),
            NIL_NAME => Some(ClassId(NIL_ID)),
            BYTE_ARRAY_NAME => Some(ClassId(BYTE_ARRAY_ID)),
            FUTURE_NAME => Some(ClassId(FUTURE_ID)),
            TUPLE1_NAME => Some(ClassId(TUPLE1_ID)),
            TUPLE2_NAME => Some(ClassId(TUPLE2_ID)),
            TUPLE3_NAME => Some(ClassId(TUPLE3_ID)),
            TUPLE4_NAME => Some(ClassId(TUPLE4_ID)),
            TUPLE5_NAME => Some(ClassId(TUPLE5_ID)),
            TUPLE6_NAME => Some(ClassId(TUPLE6_ID)),
            TUPLE7_NAME => Some(ClassId(TUPLE7_ID)),
            TUPLE8_NAME => Some(ClassId(TUPLE8_ID)),
            _ => None,
        }
    }

    pub fn builtin_function(&self, name: &str) -> Option<BuiltinFunctionId> {
        self.builtin_functions.index_of(name).map(BuiltinFunctionId)
    }

    pub fn module(&self, name: &str) -> ModuleId {
        if let Some(id) = self.module_mapping.get(name).cloned() {
            return id;
        }

        panic!("The module '{}' isn't registered in the type database", name);
    }

    pub fn class_in_module(&self, module: &str, name: &str) -> ClassId {
        if let Some(Symbol::Class(id)) = self.module(module).symbol(self, name)
        {
            id
        } else {
            panic!("The class {}::{} isn't defined", module, name)
        }
    }

    pub fn trait_in_module(&self, module: &str, name: &str) -> TraitId {
        if let Some(Symbol::Trait(id)) = self.module(module).symbol(self, name)
        {
            id
        } else {
            panic!("The trait {}::{} isn't defined", module, name)
        }
    }

    pub fn drop_trait(&self) -> TraitId {
        self.trait_in_module(DROP_MODULE, DROP_TRAIT)
    }

    pub fn number_of_modules(&self) -> usize {
        self.modules.len()
    }

    pub fn number_of_classes(&self) -> usize {
        self.classes.len()
    }

    pub fn number_of_methods(&self) -> usize {
        self.methods.len()
    }

    pub fn set_main_module(&mut self, name: ModuleName) {
        self.main_module = Some(name);
    }

    pub fn main_module(&self) -> Option<&ModuleName> {
        self.main_module.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<TypeId>(), 16);
    }

    #[test]
    fn test_type_parameter_alloc() {
        let mut db = Database::new();
        let id = TypeParameter::alloc(&mut db, "A".to_string());

        assert_eq!(id.0, 0);
        assert_eq!(&db.type_parameters[0].name, &"A".to_string());
    }

    #[test]
    fn test_type_parameter_new() {
        let param = TypeParameter::new("A".to_string());

        assert_eq!(&param.name, &"A");
        assert!(param.requirements.is_empty());
    }

    #[test]
    fn test_type_parameter_id_name() {
        let mut db = Database::new();
        let id = TypeParameter::alloc(&mut db, "A".to_string());

        assert_eq!(id.name(&db), &"A");
    }

    #[test]
    fn test_type_parameter_id_add_requirements() {
        let mut db = Database::new();
        let id = TypeParameter::alloc(&mut db, "A".to_string());
        let trait_id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let requirement = TraitInstance::new(trait_id);

        id.add_requirements(&mut db, vec![requirement]);

        assert_eq!(id.requirements(&db), vec![requirement]);
    }

    #[test]
    fn test_type_parameter_id_type_check() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let p1 = TypeParameter::alloc(&mut db, "A".to_string());
        let p2 = TypeParameter::alloc(&mut db, "B".to_string());
        let p3 = TypeParameter::alloc(&mut db, "C".to_string());
        let mut ctx = TypeContext::new(self_type);

        assert!(p1.type_check(
            &mut db,
            TypeId::TypeParameter(p2),
            &mut ctx,
            false
        ));
        assert!(!p1.type_check(
            &mut db,
            TypeId::RigidTypeParameter(p3),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_parameter_id_type_check_with_requirements() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let p1 = TypeParameter::alloc(&mut db, "A".to_string());
        let p2 = TypeParameter::alloc(&mut db, "B".to_string());

        p1.add_requirements(&mut db, vec![TraitInstance::new(to_s)]);
        p2.add_requirements(&mut db, vec![TraitInstance::new(to_s)]);

        let mut ctx = TypeContext::new(self_type);

        assert!(p1.type_check(
            &mut db,
            TypeId::TypeParameter(p2),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_arguments_assign() {
        let mut targs = TypeArguments::new();
        let mut db = Database::new();
        let param1 = TypeParameter::alloc(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "B".to_string());

        targs.assign(param1, TypeRef::Never);

        assert_eq!(targs.get(param1), Some(TypeRef::Never));
        assert_eq!(targs.get(param2), None);
        assert_eq!(targs.mapping.len(), 1);
    }

    #[test]
    fn test_trait_alloc() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );

        assert_eq!(id.0, 0);
        assert_eq!(&db.traits[0].name, &"A".to_string());
    }

    #[test]
    fn test_trait_new() {
        let trait_type =
            Trait::new("A".to_string(), ModuleId(0), Visibility::Private);

        assert_eq!(&trait_type.name, &"A");
    }

    #[test]
    fn test_trait_id_new_type_parameter() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param = id.new_type_parameter(&mut db, "A".to_string());

        assert_eq!(id.type_parameters(&db), vec![param]);
    }

    #[test]
    fn test_trait_instance_new() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins = TraitInstance::new(id);
        let index = db.traits.len() as u32 - 1;

        assert_eq!(ins.instance_of.0, index);
        assert_eq!(ins.type_arguments, 0);
    }

    #[test]
    fn test_trait_instance_generic() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins1 = TraitInstance::generic(&mut db, id, TypeArguments::new());
        let ins2 = TraitInstance::generic(&mut db, id, TypeArguments::new());
        let index = db.traits.len() as u32 - 1;

        assert_eq!(ins1.instance_of.0, index);
        assert_eq!(ins1.type_arguments, 0);

        assert_eq!(ins2.instance_of.0, index);
        assert_eq!(ins2.type_arguments, 1);
    }

    #[test]
    fn test_trait_instance_type_check_with_generic_trait_instance() {
        let mut db = Database::new();
        let trait_a = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let trait_b = Trait::alloc(
            &mut db,
            "B".to_string(),
            ModuleId(0),
            Visibility::Private,
        );

        let param1 = trait_a.new_type_parameter(&mut db, "A".to_string());

        trait_b.new_type_parameter(&mut db, "A".to_string());

        let mut ins1_args = TypeArguments::new();
        let mut ins2_args = TypeArguments::new();
        let mut ins3_args = TypeArguments::new();

        ins1_args.assign(param1, TypeRef::Any);
        ins2_args.assign(param1, TypeRef::Any);
        ins3_args.assign(param1, TypeRef::Never);

        let ins1 = TraitInstance::generic(&mut db, trait_a, ins1_args);
        let ins2 = TraitInstance::generic(&mut db, trait_a, ins2_args);
        let ins3 = TraitInstance::generic(&mut db, trait_a, ins3_args);
        let ins4 =
            TraitInstance::generic(&mut db, trait_a, TypeArguments::new());
        let ins5 =
            TraitInstance::generic(&mut db, trait_b, TypeArguments::new());

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(ins1.type_check(
            &mut db,
            TypeId::TraitInstance(ins2),
            &mut ctx,
            false
        ));
        assert!(!ins1.type_check(
            &mut db,
            TypeId::TraitInstance(ins3),
            &mut ctx,
            false
        ));
        assert!(!ins1.type_check(
            &mut db,
            TypeId::TraitInstance(ins4),
            &mut ctx,
            false
        ));
        assert!(!ins4.type_check(
            &mut db,
            TypeId::TraitInstance(ins5),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_trait_instance_type_check_with_generic_trait_as_required_trait() {
        let mut db = Database::new();
        let trait_b = Trait::alloc(
            &mut db,
            "B".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let trait_c = Trait::alloc(
            &mut db,
            "C".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins_b =
            TraitInstance::generic(&mut db, trait_b, TypeArguments::new());
        let ins_c = TypeId::TraitInstance(TraitInstance::generic(
            &mut db,
            trait_c,
            TypeArguments::new(),
        ));

        {
            let req =
                TraitInstance::generic(&mut db, trait_c, TypeArguments::new());

            trait_b.add_required_trait(&mut db, req);
        }

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(ins_b.type_check(&mut db, ins_c, &mut ctx, true));
    }

    #[test]
    fn test_trait_instance_type_check_with_regular_trait() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let requirement = TraitInstance::new(to_string);

        debug.add_required_trait(&mut db, requirement);

        let debug_ins = TraitInstance::new(debug);
        let to_string_ins =
            TypeId::TraitInstance(TraitInstance::new(to_string));
        let to_int_ins = TypeId::TraitInstance(TraitInstance::new(to_int));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(debug_ins.type_check(&mut db, to_string_ins, &mut ctx, true));
        assert!(!debug_ins.type_check(&mut db, to_int_ins, &mut ctx, true));
    }

    #[test]
    fn test_trait_instance_type_check_with_rigid_type_parameter() {
        let mut db = Database::new();
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);
        let param = TypeParameter::alloc(&mut db, "A".to_string());
        let param_ins = TypeId::RigidTypeParameter(param);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!to_s_ins.type_check(&mut db, param_ins, &mut ctx, false));
    }

    #[test]
    fn test_trait_instance_type_check_with_type_parameter() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1 = TypeParameter::alloc(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "B".to_string());
        let param3 = TypeParameter::alloc(&mut db, "C".to_string());
        let debug_ins = TraitInstance::new(debug);
        let to_string_ins = TraitInstance::new(to_string);

        debug.add_required_trait(&mut db, to_string_ins);
        param2.add_requirements(&mut db, vec![debug_ins]);
        param3.add_requirements(&mut db, vec![to_string_ins]);

        let to_int_ins = TraitInstance::new(to_int);
        let param1_ins = TypeId::TypeParameter(param1);
        let param2_ins = TypeId::TypeParameter(param2);
        let param3_ins = TypeId::TypeParameter(param3);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(debug_ins.type_check(&mut db, param1_ins, &mut ctx, true));
        assert!(debug_ins.type_check(&mut db, param2_ins, &mut ctx, true));
        assert!(debug_ins.type_check(&mut db, param3_ins, &mut ctx, true));
        assert!(!to_int_ins.type_check(&mut db, param2_ins, &mut ctx, true));
    }

    #[test]
    fn test_trait_instance_type_check_with_other_variants() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let debug_ins = TraitInstance::new(debug);
        let closure = TypeId::Closure(Closure::alloc(&mut db, false));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!debug_ins.type_check(&mut db, closure, &mut ctx, false));
    }

    #[test]
    fn test_trait_instance_format_type_with_regular_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "A".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let trait_ins = TraitInstance::new(trait_id);

        assert_eq!(format_type(&db, trait_ins), "A".to_string());
    }

    #[test]
    fn test_trait_instance_format_type_with_generic_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1 = trait_id.new_type_parameter(&mut db, "A".to_string());

        trait_id.new_type_parameter(&mut db, "B".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let trait_ins = TraitInstance::generic(&mut db, trait_id, targs);

        assert_eq!(format_type(&db, trait_ins), "ToString[Any, B]");
    }

    #[test]
    fn test_trait_instance_as_rigid_type_with_regular_trait() {
        let mut db = Database::new();
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);
        let bounds = TypeBounds::new();
        let rigid = to_s_ins.as_rigid_type(&mut db, &bounds);

        assert_eq!(rigid, to_s_ins);
    }

    #[test]
    fn test_trait_instance_as_rigid_type_with_generic_trait() {
        let mut db = Database::new();
        let to_a = Trait::alloc(
            &mut db,
            "ToArray".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1 = to_a.new_type_parameter(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "A".to_string());
        let mut args = TypeArguments::new();

        args.assign(param1, TypeRef::Owned(TypeId::TypeParameter(param2)));

        let to_a_ins = TraitInstance::generic(&mut db, to_a, args);
        let bounds = TypeBounds::new();
        let rigid = to_a_ins.as_rigid_type(&mut db, &bounds);
        let old_arg = to_a_ins.type_arguments(&db).get(param1).unwrap();
        let new_arg = rigid.type_arguments(&db).get(param1).unwrap();

        assert_ne!(old_arg, new_arg);
        assert_eq!(new_arg, TypeParameterId(1).as_owned_rigid());
    }

    #[test]
    fn test_class_alloc() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );

        assert_eq!(id.0, FIRST_USER_CLASS_ID);
        assert_eq!(
            &db.classes[FIRST_USER_CLASS_ID as usize].name,
            &"A".to_string()
        );
        assert_eq!(
            db.classes[FIRST_USER_CLASS_ID as usize].kind,
            ClassKind::Regular
        );
    }

    #[test]
    fn test_class_new() {
        let class = Class::new(
            "A".to_string(),
            ClassKind::Async,
            Visibility::Private,
            ModuleId(0),
        );

        assert_eq!(&class.name, &"A");
        assert_eq!(class.kind, ClassKind::Async);
    }

    #[test]
    fn test_class_id_name() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );

        assert_eq!(id.name(&db), &"A");
    }

    #[test]
    fn test_class_id_is_async() {
        let mut db = Database::new();
        let regular_class = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let async_class = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Async,
            Visibility::Private,
            ModuleId(0),
        );

        assert!(!regular_class.kind(&db).is_async());
        assert!(async_class.kind(&db).is_async());
    }

    #[test]
    fn test_class_id_new_type_parameter() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param = id.new_type_parameter(&mut db, "A".to_string());

        assert_eq!(id.type_parameters(&db), vec![param]);
    }

    #[test]
    fn test_class_instance_new() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins = ClassInstance::new(id);

        assert_eq!(ins.instance_of.0, FIRST_USER_CLASS_ID);
        assert_eq!(ins.type_arguments, 0);
    }

    #[test]
    fn test_class_instance_generic() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins1 = ClassInstance::generic(&mut db, id, TypeArguments::new());
        let ins2 = ClassInstance::generic(&mut db, id, TypeArguments::new());

        assert_eq!(ins1.instance_of.0, FIRST_USER_CLASS_ID);
        assert_eq!(ins1.type_arguments, 0);

        assert_eq!(ins2.instance_of.0, FIRST_USER_CLASS_ID);
        assert_eq!(ins2.type_arguments, 1);
    }

    #[test]
    fn test_class_instance_type_check_with_class_instance() {
        let mut db = Database::new();
        let cls1 = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let cls2 = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins1 = ClassInstance::new(cls1);
        let ins2 = ClassInstance::new(cls1);
        let ins3 = ClassInstance::new(cls2);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins1),
            &mut ctx,
            false
        ));
        assert!(ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins2),
            &mut ctx,
            false
        ));
        assert!(!ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins3),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_class_instance_type_check_with_generic_class_instance() {
        let mut db = Database::new();
        let class_a = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_b = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );

        let param1 = class_a.new_type_parameter(&mut db, "A".to_string());

        class_b.new_type_parameter(&mut db, "A".to_string());

        let mut ins1_args = TypeArguments::new();
        let mut ins2_args = TypeArguments::new();
        let mut ins3_args = TypeArguments::new();

        ins1_args.assign(param1, TypeRef::Any);
        ins2_args.assign(param1, TypeRef::Any);
        ins3_args.assign(param1, TypeRef::Never);

        let ins1 = ClassInstance::generic(&mut db, class_a, ins1_args);
        let ins2 = ClassInstance::generic(&mut db, class_a, ins2_args);
        let ins3 = ClassInstance::generic(&mut db, class_a, ins3_args);
        let ins4 =
            ClassInstance::generic(&mut db, class_a, TypeArguments::new());
        let ins5 =
            ClassInstance::generic(&mut db, class_b, TypeArguments::new());

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins2),
            &mut ctx,
            false
        ));
        assert!(!ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins3),
            &mut ctx,
            false
        ));
        assert!(!ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins4),
            &mut ctx,
            false
        ));
        assert!(!ins4.type_check(
            &mut db,
            TypeId::ClassInstance(ins5),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_class_instance_type_check_with_empty_type_arguments() {
        let mut db = Database::new();
        let array = Class::alloc(
            &mut db,
            "Array".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param = array.new_type_parameter(&mut db, "T".to_string());
        let ins1 = ClassInstance::generic(&mut db, array, TypeArguments::new());
        let ins2 = {
            let mut args = TypeArguments::new();

            args.assign(param, TypeRef::Any);
            ClassInstance::generic(&mut db, array, args)
        };

        let stype = TypeId::ClassInstance(ins1);
        let mut ctx = TypeContext::new(stype);

        assert!(ins1.type_check(
            &mut db,
            TypeId::ClassInstance(ins2),
            &mut ctx,
            false
        ));
        assert!(!ins2.type_check(
            &mut db,
            TypeId::ClassInstance(ins1),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_class_instance_type_check_with_trait_instance() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string_ins = TraitInstance::new(to_string);
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );

        string.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: to_string_ins,
                bounds: TypeBounds::new(),
            },
        );

        let string_ins = ClassInstance::new(string);
        let to_int_ins = TypeId::TraitInstance(TraitInstance::new(to_int));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(string_ins.type_check(
            &mut db,
            TypeId::TraitInstance(to_string_ins),
            &mut ctx,
            true
        ));

        assert!(!string_ins.type_check(&mut db, to_int_ins, &mut ctx, true));
    }

    #[test]
    fn test_class_instance_type_check_with_generic_trait_instance() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let owned_string =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(string)));
        let owned_int =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(int)));
        let equal = Trait::alloc(
            &mut db,
            "Equal".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let equal_param = equal.new_type_parameter(&mut db, "T".to_string());

        let equal_string = {
            let mut type_args = TypeArguments::new();

            type_args.assign(equal_param, owned_string);
            TraitInstance::generic(&mut db, equal, type_args)
        };

        let equal_int = {
            let mut type_args = TypeArguments::new();

            type_args.assign(equal_param, owned_int);
            TraitInstance::generic(&mut db, equal, type_args)
        };

        let equal_any = {
            let mut type_args = TypeArguments::new();

            type_args.assign(equal_param, TypeRef::Any);
            TraitInstance::generic(&mut db, equal, type_args)
        };

        string.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: equal_string,
                bounds: TypeBounds::new(),
            },
        );

        let string_ins = ClassInstance::new(string);
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        // String -> Equal[String] is OK because String implements
        // Equal[String].
        assert!(string_ins.type_check(
            &mut db,
            TypeId::TraitInstance(equal_string),
            &mut ctx,
            true
        ));

        // String -> Equal[Any] is OK, as Equal[String] is compatible with
        // Equal[Any] (but not the other way around).
        assert!(string_ins.type_check(
            &mut db,
            TypeId::TraitInstance(equal_any),
            &mut ctx,
            true
        ));

        // String -> Equal[Int] is not OK, as Equal[Int] isn't implemented by
        // String.
        assert!(!string_ins.type_check(
            &mut db,
            TypeId::TraitInstance(equal_int),
            &mut ctx,
            true
        ));
    }

    #[test]
    fn test_class_instance_type_check_with_trait_instance_with_bounds() {
        let mut db = Database::new();
        let array = Class::alloc(
            &mut db,
            "Array".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let float = Class::alloc(
            &mut db,
            "Float".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param = array.new_type_parameter(&mut db, "T".to_string());
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string_ins = TraitInstance::new(to_string);
        let mut to_string_impl = TraitImplementation {
            instance: to_string_ins,
            bounds: TypeBounds::new(),
        };

        let bound_param = TypeParameter::alloc(&mut db, "T".to_string());

        bound_param.add_requirements(&mut db, vec![to_string_ins]);
        to_string_impl.bounds.set(param, bound_param);
        array.add_trait_implementation(&mut db, to_string_impl);

        int.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: to_string_ins,
                bounds: TypeBounds::new(),
            },
        );

        let empty_array =
            ClassInstance::generic(&mut db, array, TypeArguments::new());

        let int_array = {
            let mut args = TypeArguments::new();

            args.assign(
                param,
                TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(int))),
            );

            ClassInstance::generic(&mut db, array, args)
        };

        let float_array = {
            let mut args = TypeArguments::new();

            args.assign(
                param,
                TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(
                    float,
                ))),
            );

            ClassInstance::generic(&mut db, array, args)
        };

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);
        let to_string_type = TypeId::TraitInstance(to_string_ins);

        assert!(!empty_array.type_check(
            &mut db,
            to_string_type,
            &mut ctx,
            true
        ));
        assert!(!float_array.type_check(
            &mut db,
            to_string_type,
            &mut ctx,
            true
        ));
        assert!(int_array.type_check(&mut db, to_string_type, &mut ctx, true));
    }

    #[test]
    fn test_class_instance_type_check_with_type_parameter() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string_ins = TraitInstance::new(to_string);
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_int_ins = TraitInstance::new(to_int);
        let param1 = TypeParameter::alloc(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "B".to_string());
        let param3 = TypeParameter::alloc(&mut db, "C".to_string());

        string.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: to_string_ins,
                bounds: TypeBounds::new(),
            },
        );

        param2.add_requirements(&mut db, vec![to_string_ins]);
        param3.add_requirements(&mut db, vec![to_int_ins]);

        let string_ins = ClassInstance::new(string);
        let param1_type = TypeId::TypeParameter(param1);
        let param2_type = TypeId::TypeParameter(param2);
        let param3_type = TypeId::TypeParameter(param3);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        // String -> A is OK, as A has no requirements.
        assert!(string_ins.type_check(&mut db, param1_type, &mut ctx, true));

        // String -> B is OK, as ToString is implemented by String.
        assert!(string_ins.type_check(&mut db, param2_type, &mut ctx, true));

        // String -> C is not OK, as ToInt isn't implemented.
        assert!(!string_ins.type_check(&mut db, param3_type, &mut ctx, true));
    }

    #[test]
    fn test_class_instance_type_check_with_rigid_type_parameter() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = ClassInstance::new(string);
        let param = TypeParameter::alloc(&mut db, "A".to_string());
        let param_ins = TypeId::RigidTypeParameter(param);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!string_ins.type_check(&mut db, param_ins, &mut ctx, false));
    }

    #[test]
    fn test_class_instance_type_check_with_function() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let closure = TypeId::Closure(Closure::alloc(&mut db, false));
        let string_ins = ClassInstance::new(string);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!string_ins.type_check(&mut db, closure, &mut ctx, false));
    }

    #[test]
    fn test_class_instance_as_rigid_type_with_regular_trait() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = ClassInstance::new(string);
        let bounds = TypeBounds::new();
        let rigid = string_ins.as_rigid_type(&mut db, &bounds);

        assert_eq!(rigid, string_ins);
    }

    #[test]
    fn test_class_instance_as_rigid_type_with_generic_trait() {
        let mut db = Database::new();
        let array = Class::alloc(
            &mut db,
            "Array".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = array.new_type_parameter(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "A".to_string());
        let mut args = TypeArguments::new();

        args.assign(param1, TypeRef::Owned(TypeId::TypeParameter(param2)));

        let to_a_ins = ClassInstance::generic(&mut db, array, args);
        let bounds = TypeBounds::new();
        let rigid = to_a_ins.as_rigid_type(&mut db, &bounds);
        let old_arg = to_a_ins.type_arguments(&db).get(param1).unwrap();
        let new_arg = rigid.type_arguments(&db).get(param1).unwrap();

        assert_ne!(old_arg, new_arg);
        assert_eq!(new_arg, TypeParameterId(1).as_owned_rigid());
    }

    #[test]
    fn test_method_alloc() {
        let mut db = Database::new();
        let id = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Moving,
        );

        assert_eq!(id.0, 0);
        assert_eq!(&db.methods[0].name, &"foo".to_string());
        assert_eq!(db.methods[0].kind, MethodKind::Moving);
    }

    #[test]
    fn test_method_id_named_type() {
        let mut db = Database::new();
        let method = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let param = method.new_type_parameter(&mut db, "A".to_string());

        assert_eq!(
            method.named_type(&db, "A"),
            Some(Symbol::TypeParameter(param))
        );
    }

    #[test]
    fn test_method_id_format_type_with_instance_method() {
        let mut db = Database::new();
        let class_a = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_b = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_c = Class::alloc(
            &mut db,
            "C".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_d = Class::alloc(
            &mut db,
            "D".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        let ins_a =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_a)));

        let ins_b =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_b)));

        let ins_c =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_c)));

        let ins_d =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_d)));

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b);
        block.set_throw_type(&mut db, ins_c);
        block.set_return_type(&mut db, ins_d);

        assert_eq!(format_type(&db, block), "fn foo (a: A, b: B) !! C -> D");
    }

    #[test]
    fn test_method_id_format_type_with_moving_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Moving,
        );

        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn move foo -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_type_parameters() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        block.new_type_parameter(&mut db, "A".to_string());
        block.new_type_parameter(&mut db, "B".to_string());
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn static foo [A, B] -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_static_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::Any,
            TypeRef::Any,
        );
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn static foo (a: Any) -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_async_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Async,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::Any,
            TypeRef::Any,
        );
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn async foo (a: Any) -> Any");
    }

    #[test]
    fn test_method_id_type_check_with_different_name() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "b".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_different_visibility() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Public,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_different_kind() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_different_param_count() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        m2.new_type_parameter(&mut db, "T".to_string());
        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_incompatible_params() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);

        m1.new_type_parameter(&mut db, "T".to_string());

        m2.new_type_parameter(&mut db, "T".to_string())
            .add_requirements(&mut db, vec![to_s_ins]);

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_incompatible_arg_types() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m3 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins =
            TypeRef::Owned(TypeId::TraitInstance(TraitInstance::new(to_s)));

        m1.new_argument(&mut db, "a".to_string(), to_s_ins, to_s_ins);
        m3.new_argument(&mut db, "a".to_string(), to_s_ins, to_s_ins);

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);
        m3.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
        assert!(!m2.type_check(&mut db, m3, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_incompatible_arg_names() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        m1.new_argument(&mut db, "a".to_string(), TypeRef::Any, TypeRef::Any);
        m2.new_argument(&mut db, "b".to_string(), TypeRef::Any, TypeRef::Any);

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_incompatible_throw_type() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        m1.set_throw_type(&mut db, TypeRef::Any);
        m1.set_return_type(&mut db, TypeRef::Any);

        m2.set_throw_type(&mut db, TypeRef::Never);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_incompatible_return_type() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        m1.set_return_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Never);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(!m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_method_id_type_check_with_compatible_method() {
        let mut db = Database::new();
        let m1 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );
        let m2 = Method::alloc(
            &mut db,
            ModuleId(0),
            "a".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        m1.new_type_parameter(&mut db, "T".to_string());
        m1.new_argument(&mut db, "a".to_string(), TypeRef::Any, TypeRef::Any);
        m1.set_throw_type(&mut db, TypeRef::Any);
        m1.set_return_type(&mut db, TypeRef::Any);

        m2.new_type_parameter(&mut db, "T".to_string());
        m2.new_argument(&mut db, "a".to_string(), TypeRef::Any, TypeRef::Any);
        m2.set_throw_type(&mut db, TypeRef::Any);
        m2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(m1.type_check(&mut db, m2, &mut ctx));
    }

    #[test]
    fn test_module_alloc() {
        let mut db = Database::new();
        let name = ModuleName::new("foo");
        let id = Module::alloc(&mut db, name.clone(), "foo.inko".into());

        assert_eq!(id.0, 0);
        assert_eq!(&db.modules[0].name, &name);
        assert_eq!(&db.modules[0].file, &PathBuf::from("foo.inko"));
    }

    #[test]
    fn test_module_id_file() {
        let mut db = Database::new();
        let id = Module::alloc(
            &mut db,
            ModuleName::new("foo"),
            PathBuf::from("test.inko"),
        );

        assert_eq!(id.file(&db), PathBuf::from("test.inko"));
    }

    #[test]
    fn test_module_id_symbol() {
        let mut db = Database::new();
        let id = Module::alloc(
            &mut db,
            ModuleName::new("foo"),
            PathBuf::from("test.inko"),
        );

        id.new_symbol(&mut db, "A".to_string(), Symbol::Module(id));

        assert_eq!(id.symbol(&db, "A"), Some(Symbol::Module(id)));
    }

    #[test]
    fn test_module_id_symbols() {
        let mut db = Database::new();
        let id = Module::alloc(
            &mut db,
            ModuleName::new("foo"),
            PathBuf::from("test.inko"),
        );

        id.new_symbol(&mut db, "A".to_string(), Symbol::Module(id));

        assert_eq!(
            id.symbols(&db),
            vec![("A".to_string(), Symbol::Module(id))]
        );
    }

    #[test]
    fn test_module_id_symbol_exists() {
        let mut db = Database::new();
        let id = Module::alloc(
            &mut db,
            ModuleName::new("foo"),
            PathBuf::from("test.inko"),
        );

        id.new_symbol(&mut db, "A".to_string(), Symbol::Module(id));

        assert!(id.symbol_exists(&db, "A"));
        assert!(!id.symbol_exists(&db, "B"));
    }

    #[test]
    fn test_function_closure() {
        let mut db = Database::new();
        let id = Closure::alloc(&mut db, false);

        assert_eq!(id.0, 0);
    }

    #[test]
    fn test_closure_id_format_type_never_throws() {
        let mut db = Database::new();
        let block = Closure::alloc(&mut db, false);

        block.set_throw_type(&mut db, TypeRef::Never);
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn -> Any");
    }

    #[test]
    fn test_closure_id_format_type_never_returns() {
        let mut db = Database::new();
        let block = Closure::alloc(&mut db, false);

        block.set_return_type(&mut db, TypeRef::Never);

        assert_eq!(format_type(&db, block), "fn -> Never");
    }

    #[test]
    fn test_closure_id_type_check_with_empty_closure() {
        let mut db = Database::new();
        let closure1 = Closure::alloc(&mut db, false);
        let closure2 = Closure::alloc(&mut db, false);

        closure1.set_return_type(&mut db, TypeRef::Any);
        closure2.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(closure1.type_check(
            &mut db,
            TypeId::Closure(closure2),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_closure_id_type_check_with_arguments() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(string)));
        let closure1 = Closure::alloc(&mut db, false);
        let closure2 = Closure::alloc(&mut db, false);
        let closure3 = Closure::alloc(&mut db, false);

        closure1.new_argument(&mut db, "a".to_string(), string_ins, string_ins);
        closure1.set_return_type(&mut db, TypeRef::Any);

        closure2.new_argument(&mut db, "x".to_string(), string_ins, string_ins);
        closure2.set_return_type(&mut db, TypeRef::Any);

        closure3.new_argument(&mut db, "a".to_string(), string_ins, string_ins);
        closure3.new_argument(&mut db, "b".to_string(), string_ins, string_ins);
        closure3.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(closure1.type_check(
            &mut db,
            TypeId::Closure(closure2),
            &mut ctx,
            false
        ));
        assert!(!closure1.type_check(
            &mut db,
            TypeId::Closure(closure3),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_closure_id_type_check_with_throw_type() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(string)));
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(int)));
        let closure1 = Closure::alloc(&mut db, false);
        let closure2 = Closure::alloc(&mut db, false);
        let closure3 = Closure::alloc(&mut db, false);
        let closure4 = Closure::alloc(&mut db, false);

        closure1.set_throw_type(&mut db, string_ins);
        closure1.set_return_type(&mut db, TypeRef::Any);

        closure2.set_throw_type(&mut db, string_ins);
        closure2.set_return_type(&mut db, TypeRef::Any);

        closure3.set_throw_type(&mut db, int_ins);
        closure3.set_return_type(&mut db, TypeRef::Any);

        closure4.set_return_type(&mut db, TypeRef::Any);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(closure1.type_check(
            &mut db,
            TypeId::Closure(closure2),
            &mut ctx,
            false
        ));
        assert!(closure4.type_check(
            &mut db,
            TypeId::Closure(closure1),
            &mut ctx,
            false
        ));
        assert!(!closure1.type_check(
            &mut db,
            TypeId::Closure(closure3),
            &mut ctx,
            false
        ));
        assert!(!closure1.type_check(
            &mut db,
            TypeId::Closure(closure4),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_closure_id_type_check_with_return_type() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(string)));
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(int)));
        let closure1 = Closure::alloc(&mut db, false);
        let closure2 = Closure::alloc(&mut db, false);
        let closure3 = Closure::alloc(&mut db, false);
        let closure4 = Closure::alloc(&mut db, false);

        closure1.set_return_type(&mut db, string_ins);
        closure2.set_return_type(&mut db, string_ins);
        closure3.set_return_type(&mut db, int_ins);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(closure1.type_check(
            &mut db,
            TypeId::Closure(closure2),
            &mut ctx,
            false
        ));
        assert!(!closure1.type_check(
            &mut db,
            TypeId::Closure(closure3),
            &mut ctx,
            false
        ));
        assert!(!closure1.type_check(
            &mut db,
            TypeId::Closure(closure4),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_closure_id_type_check_with_type_parameter() {
        let mut db = Database::new();
        let closure = Closure::alloc(&mut db, false);
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string_ins = TraitInstance::new(to_string);
        let param1 = TypeParameter::alloc(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "B".to_string());

        param2.add_requirements(&mut db, vec![to_string_ins]);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let self_type = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(self_type);

        assert!(closure.type_check(
            &mut db,
            TypeId::TypeParameter(param1),
            &mut ctx,
            false
        ));
        assert!(!closure.type_check(
            &mut db,
            TypeId::TypeParameter(param2),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_closure_id_as_rigid_type_with_regular_function() {
        let mut db = Database::new();
        let closure = Closure::alloc(&mut db, false);
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);
        let to_s_type = TypeRef::Owned(TypeId::TraitInstance(to_s_ins));

        closure.new_argument(&mut db, "a".to_string(), to_s_type, to_s_type);

        let bounds = TypeBounds::new();
        let new_closure = closure.as_rigid_type(&mut db, &bounds);

        assert_eq!(new_closure, ClosureId(1));
    }

    #[test]
    fn test_closure_id_as_rigid_type_with_generic_function() {
        let mut db = Database::new();
        let closure = Closure::alloc(&mut db, false);
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let param_type = TypeRef::Owned(TypeId::TypeParameter(param));

        closure.new_argument(&mut db, "a".to_string(), param_type, param_type);
        closure.set_throw_type(&mut db, param_type);
        closure.set_return_type(&mut db, param_type);

        let bounds = TypeBounds::new();
        let new_closure = closure.as_rigid_type(&mut db, &bounds);

        assert_ne!(closure, new_closure);

        let new_arg = new_closure.get(&db).arguments.get("a").unwrap();

        assert_eq!(new_arg.value_type, param_type);
        assert_eq!(
            new_closure.throw_type(&db),
            TypeParameterId(0).as_owned_rigid()
        );
        assert_eq!(
            new_closure.return_type(&db),
            TypeParameterId(0).as_owned_rigid()
        );
    }

    #[test]
    fn test_type_ref_type_check_with_owned() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let string_typ = TypeRef::Owned(string_ins);
        let string_ref_typ = TypeRef::Ref(string_ins);
        let int_typ = TypeRef::Owned(int_ins);

        let mut ctx = TypeContext::new(string_ins);
        assert!(string_typ.type_check(&mut db, string_typ, &mut ctx, false));

        let mut ctx = TypeContext::new(string_ins);
        assert!(string_typ.type_check(&mut db, TypeRef::Any, &mut ctx, false));

        let mut ctx = TypeContext::new(string_ins);
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::OwnedSelf,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(!string_typ.type_check(
            &mut db,
            string_ref_typ,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(!string_typ.type_check(
            &mut db,
            TypeRef::RefSelf,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(!string_typ.type_check(
            &mut db,
            TypeRef::Unknown,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(!string_typ.type_check(&mut db, int_typ, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_ref() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let string_typ = TypeRef::Ref(string_ins);
        let mut ctx_int = TypeContext::new(int_ins);
        let mut ctx_str = TypeContext::new(string_ins);

        assert!(string_typ.type_check(
            &mut db,
            string_typ,
            &mut ctx_int,
            false
        ));
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx_int,
            false
        ));
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::RefSelf,
            &mut ctx_str,
            false
        ));
        assert!(!string_typ.type_check(
            &mut db,
            TypeRef::Owned(string_ins),
            &mut ctx_int,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_mut() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let string_typ = TypeRef::Mut(string_ins);
        let mut ctx_int = TypeContext::new(int_ins);
        let mut ctx_str = TypeContext::new(string_ins);

        assert!(string_typ.type_check(
            &mut db,
            string_typ,
            &mut ctx_int,
            false
        ));
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx_int,
            false
        ));
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::RefSelf,
            &mut ctx_str,
            false
        ));
        assert!(string_typ.type_check(
            &mut db,
            TypeRef::MutSelf,
            &mut ctx_str,
            false
        ));
        assert!(!string_typ.type_check(
            &mut db,
            TypeRef::Owned(string_ins),
            &mut ctx_int,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_mut_trait() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let to_string = TraitInstance::new(Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        ));

        string.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: to_string,
                bounds: TypeBounds::new(),
            },
        );

        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let string_typ = TypeRef::Mut(string_ins);
        let mut ctx = TypeContext::new(string_ins);

        assert!(string_typ.type_check(
            &mut db,
            TypeRef::Ref(TypeId::TraitInstance(to_string)),
            &mut ctx,
            true
        ));

        assert!(!string_typ.type_check(
            &mut db,
            TypeRef::Mut(TypeId::TraitInstance(to_string)),
            &mut ctx,
            true
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_infer() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let param_ins = TypeId::TypeParameter(param);
        let param_typ = TypeRef::Infer(param_ins);

        let mut ctx = TypeContext::new(int_ins);
        assert!(param_typ.type_check(&mut db, param_typ, &mut ctx, false));

        let mut ctx = TypeContext::new(int_ins);
        assert!(param_typ.type_check(&mut db, TypeRef::Error, &mut ctx, false));

        let mut ctx = TypeContext::new(param_ins);
        assert!(!param_typ.type_check(
            &mut db,
            TypeRef::RefSelf,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(int_ins);
        assert!(!param_typ.type_check(
            &mut db,
            TypeRef::Owned(param_ins),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_never() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(TypeRef::Never.type_check(
            &mut db,
            TypeRef::Never,
            &mut ctx,
            false
        ));
        assert!(TypeRef::Never.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));
        assert!(TypeRef::Never.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_any() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(string)));
        let param = TypeRef::Owned(TypeId::TypeParameter(
            TypeParameter::alloc(&mut db, "T".to_string()),
        ));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(TypeRef::Any.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));
        assert!(TypeRef::Any.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::Any.type_check(&mut db, param, &mut ctx, false));
        assert!(!TypeRef::Any.type_check(
            &mut db,
            TypeRef::OwnedSelf,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::Any.type_check(
            &mut db,
            TypeRef::RefSelf,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::Any.type_check(&mut db, string_ins, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_owned_self() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));

        let mut ctx = TypeContext::new(string_ins);
        assert!(TypeRef::OwnedSelf.type_check(
            &mut db,
            TypeRef::Owned(string_ins),
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(TypeRef::OwnedSelf.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(TypeRef::OwnedSelf.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));

        let mut ctx = TypeContext::new(string_ins);
        assert!(!TypeRef::OwnedSelf.type_check(
            &mut db,
            TypeRef::Ref(string_ins),
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_ref_self() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let string_ref = TypeRef::Ref(string_ins);
        let mut ctx = TypeContext::new(string_ins);

        assert!(
            TypeRef::RefSelf.type_check(&mut db, string_ref, &mut ctx, false)
        );
        assert!(TypeRef::RefSelf.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::RefSelf.type_check(
            &mut db,
            TypeRef::OwnedSelf,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::RefSelf.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));
        assert!(!TypeRef::RefSelf.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_error() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(TypeRef::Error.type_check(
            &mut db,
            TypeRef::Error,
            &mut ctx,
            false
        ));
        assert!(TypeRef::Error.type_check(
            &mut db,
            TypeRef::Never,
            &mut ctx,
            false
        ));
        assert!(TypeRef::Error.type_check(
            &mut db,
            TypeRef::Any,
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_ref_type_check_with_unknown() {
        let mut db = Database::new();
        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);
        let unknown = TypeRef::Unknown;

        assert!(!unknown.type_check(&mut db, unknown, &mut ctx, false));
        assert!(!unknown.type_check(&mut db, TypeRef::Error, &mut ctx, false));
        assert!(!unknown.type_check(&mut db, TypeRef::Never, &mut ctx, false));
        assert!(!unknown.type_check(&mut db, TypeRef::Any, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_type_parameter() {
        let mut db = Database::new();
        let param1 = TypeParameter::alloc(&mut db, "A".to_string());
        let param2 = TypeParameter::alloc(&mut db, "B".to_string());
        let trait_id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins = TypeId::TraitInstance(TraitInstance::new(trait_id));
        let ins_type = TypeRef::Owned(ins);

        param1.add_requirements(&mut db, vec![TraitInstance::new(trait_id)]);

        {
            let mut ctx = TypeContext::new(ins);

            assert!(!ins_type
                .is_compatible_with_type_parameter(&mut db, param1, &mut ctx));
        }

        {
            let mut ctx = TypeContext::new(ins);

            assert!(ins_type
                .is_compatible_with_type_parameter(&mut db, param2, &mut ctx));
        }

        {
            let mut ctx = TypeContext::new(ins);

            assert!(TypeRef::OwnedSelf
                .is_compatible_with_type_parameter(&mut db, param2, &mut ctx));
        }

        {
            let mut ctx = TypeContext::new(ins);

            assert!(TypeRef::RefSelf
                .is_compatible_with_type_parameter(&mut db, param2, &mut ctx));
        }

        {
            let mut ctx = TypeContext::new(ins);

            assert!(TypeRef::Owned(TypeId::TypeParameter(param1))
                .type_check(&mut db, ins_type, &mut ctx, false));
        }
    }

    #[test]
    fn test_type_ref_type_check_with_assigned_type_parameter() {
        let mut db = Database::new();
        let param = TypeRef::Owned(TypeId::TypeParameter(
            TypeParameter::alloc(&mut db, "A".to_string()),
        ));
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_i = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);
        let to_i_ins = TraitInstance::new(to_i);
        let typ1 = TypeRef::Owned(TypeId::TraitInstance(to_s_ins));
        let typ2 = TypeRef::Owned(TypeId::TraitInstance(to_i_ins));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(typ1.type_check(&mut db, param, &mut ctx, false));
        assert_eq!(ctx.type_arguments.mapping.len(), 1);
        assert!(!typ2.type_check(&mut db, param, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_implements_trait_with_class_instance() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string_ins = TraitInstance::new(to_string);
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let mut ctx = TypeContext::new(string_ins);

        string.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: to_string_ins,
                bounds: TypeBounds::new(),
            },
        );

        assert!(TypeRef::Owned(string_ins).implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx
        ));
        assert!(TypeRef::Ref(string_ins).implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx
        ));
        assert!(TypeRef::OwnedSelf.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx
        ));
        assert!(TypeRef::RefSelf.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx
        ));
    }

    #[test]
    fn test_type_ref_implements_trait_with_trait_instance() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_foo = Trait::alloc(
            &mut db,
            "ToFoo".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_s_ins = TraitInstance::new(to_s);
        let debug_ins = TypeId::TraitInstance(TraitInstance::new(debug));
        let to_foo_ins = TypeId::TraitInstance(TraitInstance::new(to_foo));

        debug.add_required_trait(&mut db, to_s_ins);

        {
            let req = TraitInstance::new(debug);

            to_foo.add_required_trait(&mut db, req);
        }

        let mut ctx = TypeContext::new(debug_ins);

        assert!(TypeRef::Owned(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
        assert!(TypeRef::Owned(to_foo_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
        assert!(TypeRef::Ref(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
        assert!(TypeRef::Infer(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
        assert!(TypeRef::OwnedSelf
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
        assert!(TypeRef::RefSelf
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx));
    }

    #[test]
    fn test_type_ref_implements_trait_with_type_parameter() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_foo = Trait::alloc(
            &mut db,
            "ToFoo".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let to_string_ins = TraitInstance::new(to_string);
        let debug_ins = TraitInstance::new(debug);
        let to_foo_ins = TraitInstance::new(to_foo);
        let param_ins = TypeRef::Owned(TypeId::TypeParameter(param));

        debug.add_required_trait(&mut db, to_string_ins);
        param.add_requirements(&mut db, vec![debug_ins]);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(
            param_ins.implements_trait_instance(&mut db, debug_ins, &mut ctx)
        );
        assert!(param_ins.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx
        ));
        assert!(
            !param_ins.implements_trait_instance(&mut db, to_foo_ins, &mut ctx)
        );
    }

    #[test]
    fn test_type_ref_implements_trait_with_other_variants() {
        let mut db = Database::new();
        let trait_type = Trait::alloc(
            &mut db,
            "ToA".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins = TraitInstance::new(trait_type);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(!TypeRef::Any.implements_trait_instance(&mut db, ins, &mut ctx));
        assert!(
            !TypeRef::Unknown.implements_trait_instance(&mut db, ins, &mut ctx)
        );
        assert!(
            TypeRef::Error.implements_trait_instance(&mut db, ins, &mut ctx)
        );
        assert!(
            TypeRef::Never.implements_trait_instance(&mut db, ins, &mut ctx)
        );
    }

    #[test]
    fn test_type_ref_type_name() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let param = TypeId::TypeParameter(TypeParameter::alloc(
            &mut db,
            "T".to_string(),
        ));

        assert_eq!(
            format_type(&db, TypeRef::Owned(string_ins)),
            "String".to_string()
        );
        assert_eq!(format_type(&db, TypeRef::Infer(param)), "T".to_string());
        assert_eq!(
            format_type(&db, TypeRef::Ref(string_ins)),
            "ref String".to_string()
        );
        assert_eq!(format_type(&db, TypeRef::Never), "Never".to_string());
        assert_eq!(format_type(&db, TypeRef::Any), "Any".to_string());
        assert_eq!(format_type(&db, TypeRef::OwnedSelf), "Self".to_string());
        assert_eq!(format_type(&db, TypeRef::RefSelf), "ref Self".to_string());
        assert_eq!(format_type(&db, TypeRef::Error), "<error>".to_string());
        assert_eq!(format_type(&db, TypeRef::Unknown), "<unknown>".to_string());
    }

    #[test]
    fn test_type_id_named_type_with_class() {
        let mut db = Database::new();
        let array = Class::alloc(
            &mut db,
            "Array".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param = array.new_type_parameter(&mut db, "T".to_string());

        assert_eq!(
            TypeId::Class(array).named_type(&db, "T"),
            Some(Symbol::TypeParameter(param))
        );
    }

    #[test]
    fn test_type_id_named_type_with_trait() {
        let mut db = Database::new();
        let to_array = Trait::alloc(
            &mut db,
            "ToArray".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param = to_array.new_type_parameter(&mut db, "T".to_string());

        assert_eq!(
            TypeId::Trait(to_array).named_type(&db, "T"),
            Some(Symbol::TypeParameter(param))
        );
    }

    #[test]
    fn test_type_id_named_type_with_module() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let module =
            Module::alloc(&mut db, ModuleName::new("foo"), "foo.inko".into());

        let symbol = Symbol::Class(string);
        let type_id = TypeId::Module(module);

        module.new_symbol(&mut db, "String".to_string(), symbol);

        assert_eq!(type_id.named_type(&db, "String"), Some(symbol));
        assert!(type_id.named_type(&db, "Foo").is_none());
    }

    #[test]
    fn test_type_id_named_type_with_class_instance() {
        let mut db = Database::new();
        let array = Class::alloc(
            &mut db,
            "Array".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param = array.new_type_parameter(&mut db, "T".to_string());
        let ins = TypeId::ClassInstance(ClassInstance::generic(
            &mut db,
            array,
            TypeArguments::new(),
        ));

        assert_eq!(
            ins.named_type(&db, "T"),
            Some(Symbol::TypeParameter(param))
        );
        assert!(ins.named_type(&db, "E").is_none());
    }

    #[test]
    fn test_type_id_named_type_with_trait_instance() {
        let mut db = Database::new();
        let to_array = Trait::alloc(
            &mut db,
            "ToArray".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param = to_array.new_type_parameter(&mut db, "T".to_string());
        let ins = TypeId::TraitInstance(TraitInstance::generic(
            &mut db,
            to_array,
            TypeArguments::new(),
        ));

        assert_eq!(
            ins.named_type(&db, "T"),
            Some(Symbol::TypeParameter(param))
        );
        assert!(ins.named_type(&db, "E").is_none());
    }

    #[test]
    fn test_type_id_named_type_with_type_parameter() {
        let mut db = Database::new();
        let param = TypeId::TypeParameter(TypeParameter::alloc(
            &mut db,
            "T".to_string(),
        ));

        assert!(param.named_type(&db, "T").is_none());
    }

    #[test]
    fn test_type_id_named_type_with_function() {
        let mut db = Database::new();
        let block = TypeId::Closure(Closure::alloc(&mut db, false));

        assert!(block.named_type(&db, "T").is_none());
    }

    #[test]
    fn test_type_ref_type_check_with_class() {
        let mut db = Database::new();
        let typ1 = TypeRef::Owned(TypeId::Class(ClassId(0)));
        let typ2 = TypeRef::Owned(TypeId::Class(ClassId(1)));
        let typ3 = TypeRef::Owned(TypeId::Trait(TraitId(0)));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(typ1.type_check(&mut db, typ1, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ2, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ3, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_trait() {
        let mut db = Database::new();
        let typ1 = TypeRef::Owned(TypeId::Trait(TraitId(0)));
        let typ2 = TypeRef::Owned(TypeId::Trait(TraitId(1)));
        let typ3 = TypeRef::Owned(TypeId::Class(ClassId(0)));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(typ1.type_check(&mut db, typ1, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ2, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ3, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_module() {
        let mut db = Database::new();
        let typ1 = TypeRef::Owned(TypeId::Module(ModuleId(0)));
        let typ2 = TypeRef::Owned(TypeId::Module(ModuleId(1)));
        let typ3 = TypeRef::Owned(TypeId::Class(ClassId(0)));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(typ1.type_check(&mut db, typ1, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ2, &mut ctx, false));
        assert!(!typ1.type_check(&mut db, typ3, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_class_instance() {
        let mut db = Database::new();
        let cls1 = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let cls2 = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins1 =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(cls1)));
        let ins2 =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(cls1)));
        let ins3 =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(cls2)));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(ins1.type_check(&mut db, ins1, &mut ctx, false));
        assert!(ins1.type_check(&mut db, ins2, &mut ctx, false));
        assert!(!ins1.type_check(&mut db, ins3, &mut ctx, false));
    }

    #[test]
    fn test_type_ref_type_check_with_trait_instance() {
        let mut db = Database::new();
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let requirement = TraitInstance::new(to_string);

        debug.add_required_trait(&mut db, requirement);

        let debug_ins =
            TypeRef::Owned(TypeId::TraitInstance(TraitInstance::new(debug)));
        let to_string_ins = TypeRef::Owned(TypeId::TraitInstance(
            TraitInstance::new(to_string),
        ));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(debug_ins.type_check(&mut db, to_string_ins, &mut ctx, true));
    }

    #[test]
    fn test_type_ref_type_check_with_function() {
        let mut db = Database::new();
        let closure1 = Closure::alloc(&mut db, false);
        let closure2 = Closure::alloc(&mut db, false);

        closure1.set_return_type(&mut db, TypeRef::Any);
        closure2.set_return_type(&mut db, TypeRef::Any);

        let closure1_type = TypeRef::Owned(TypeId::Closure(closure1));
        let closure2_type = TypeRef::Owned(TypeId::Closure(closure2));

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(closure1_type.type_check(
            &mut db,
            closure2_type,
            &mut ctx,
            false
        ));
    }

    #[test]
    fn test_type_id_format_type_with_class() {
        let mut db = Database::new();
        let id = TypeId::Class(Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        ));

        assert_eq!(format_type(&db, id), "String");
    }

    #[test]
    fn test_type_id_format_type_with_trait() {
        let mut db = Database::new();
        let id = TypeId::Trait(Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        ));

        assert_eq!(format_type(&db, id), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_module() {
        let mut db = Database::new();
        let id = TypeId::Module(Module::alloc(
            &mut db,
            ModuleName::new("foo::bar"),
            "foo/bar.inko".into(),
        ));

        assert_eq!(format_type(&db, id), "foo::bar");
    }

    #[test]
    fn test_type_id_format_type_with_class_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins = TypeId::ClassInstance(ClassInstance::new(id));

        assert_eq!(format_type(&db, ins), "String");
    }

    #[test]
    fn test_type_id_format_type_with_tuple_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "MyTuple".to_string(),
            ClassKind::Tuple,
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = id.new_type_parameter(&mut db, "A".to_string());
        let param2 = id.new_type_parameter(&mut db, "B".to_string());
        let mut args = TypeArguments::new();

        args.assign(param1, TypeRef::Any);
        args.assign(param2, TypeRef::Never);

        let ins =
            TypeId::ClassInstance(ClassInstance::generic(&mut db, id, args));

        assert_eq!(format_type(&db, ins), "(Any, Never)");
    }

    #[test]
    fn test_type_id_format_type_with_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let ins = TypeId::TraitInstance(TraitInstance::new(id));

        assert_eq!(format_type(&db, ins), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_generic_class_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "Future".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let ins =
            TypeId::ClassInstance(ClassInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "Future[Any, E]");
    }

    #[test]
    fn test_type_id_format_type_with_generic_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToFoo".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let ins =
            TypeId::TraitInstance(TraitInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "ToFoo[Any, E]");
    }

    #[test]
    fn test_type_id_format_type_with_type_parameter() {
        let mut db = Database::new();
        let param1 = TypeParameter::alloc(&mut db, "T".to_string());
        let param2 = TypeParameter::alloc(&mut db, "T".to_string());
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1_ins = TypeId::TypeParameter(param1);
        let param2_ins = TypeId::TypeParameter(param2);
        let to_string_ins = TraitInstance::new(to_string);
        let to_int_ins = TraitInstance::new(to_int);

        param1.add_requirements(&mut db, vec![to_string_ins]);
        param2.add_requirements(&mut db, vec![to_string_ins, to_int_ins]);

        assert_eq!(format_type(&db, param1_ins), "T: ToString");
        assert_eq!(format_type(&db, param2_ins), "T: ToString + ToInt");
    }

    #[test]
    fn test_type_id_format_type_with_rigid_type_parameter() {
        let mut db = Database::new();
        let param1 = TypeParameter::alloc(&mut db, "T".to_string());
        let param2 = TypeParameter::alloc(&mut db, "T".to_string());
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let to_int = Trait::alloc(
            &mut db,
            "ToInt".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let param1_ins = TypeId::RigidTypeParameter(param1);
        let param2_ins = TypeId::RigidTypeParameter(param2);
        let to_string_ins = TraitInstance::new(to_string);
        let to_int_ins = TraitInstance::new(to_int);

        param1.add_requirements(&mut db, vec![to_string_ins]);
        param2.add_requirements(&mut db, vec![to_string_ins, to_int_ins]);

        assert_eq!(format_type(&db, param1_ins), "T: ToString");
        assert_eq!(format_type(&db, param2_ins), "T: ToString + ToInt");
    }

    #[test]
    fn test_type_id_format_type_with_closure() {
        let mut db = Database::new();
        let class_a = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_b = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_c = Class::alloc(
            &mut db,
            "C".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_d = Class::alloc(
            &mut db,
            "D".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let block = Closure::alloc(&mut db, true);

        let ins_a =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_a)));

        let ins_b =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_b)));

        let ins_c =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_c)));

        let ins_d =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_d)));

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b);
        block.set_throw_type(&mut db, ins_c);
        block.set_return_type(&mut db, ins_d);

        let block_ins = TypeId::Closure(block);

        assert_eq!(format_type(&db, block_ins), "fn move (A, B) !! C -> D");
    }

    #[test]
    fn test_type_id_implements_trait_with_other_variants() {
        let mut db = Database::new();
        let closure = TypeId::Closure(Closure::alloc(&mut db, false));
        let debug = Trait::alloc(
            &mut db,
            "Debug".to_string(),
            ModuleId(0),
            Visibility::Private,
        );
        let debug_ins = TraitInstance::new(debug);

        let int = Class::alloc(
            &mut db,
            "Int".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let int_ins = TypeId::ClassInstance(ClassInstance::new(int));
        let mut ctx = TypeContext::new(int_ins);

        assert!(
            !closure.implements_trait_instance(&mut db, debug_ins, &mut ctx)
        );
    }

    #[test]
    fn test_database_new() {
        let db = Database::new();

        assert_eq!(&db.classes[0].name, INT_NAME);
        assert_eq!(&db.classes[1].name, FLOAT_NAME);
        assert_eq!(&db.classes[2].name, STRING_NAME);
        assert_eq!(&db.classes[3].name, ARRAY_NAME);
        assert_eq!(&db.classes[4].name, BOOLEAN_NAME);
        assert_eq!(&db.classes[5].name, NIL_NAME);
        assert_eq!(&db.classes[6].name, BYTE_ARRAY_NAME);
        assert_eq!(&db.classes[7].name, FUTURE_NAME);
    }

    #[test]
    fn test_database_module() {
        let mut db = Database::new();
        let name = ModuleName::new("foo");
        let id = Module::alloc(&mut db, name, "foo.inko".into());

        assert_eq!(db.module("foo"), id);
    }

    #[test]
    #[should_panic]
    fn test_database_invalid_module() {
        let db = Database::new();

        db.module("foo");
    }

    #[test]
    fn test_type_placeholder_id_assign() {
        let mut db = Database::new();
        let p1 = TypePlaceholder::alloc(&mut db);
        let p2 = TypePlaceholder::alloc(&mut db);

        p1.assign(&db, TypeRef::Any);
        p2.assign(&db, TypeRef::Placeholder(p2));

        assert_eq!(p1.value(&db), Some(TypeRef::Any));
        assert!(p2.value(&db).is_none());
    }
}
