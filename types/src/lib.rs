//! Structures for the various Inko types.
#![cfg_attr(feature = "cargo-clippy", allow(clippy::new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]

#[cfg(test)]
pub mod test;

pub mod check;
pub mod collections;
pub mod either;
pub mod format;
pub mod module_name;
pub mod resolve;

use crate::collections::IndexMap;
use crate::module_name::ModuleName;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const INT_ID: u32 = 0;
pub const FLOAT_ID: u32 = 1;
pub const STRING_ID: u32 = 2;
pub const ARRAY_ID: u32 = 3;
pub const BOOLEAN_ID: u32 = 4;
pub const NIL_ID: u32 = 5;
pub const BYTE_ARRAY_ID: u32 = 6;
pub const CHANNEL_ID: u32 = 7;

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
const CHANNEL_NAME: &str = "Channel";

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

/// A type inference placeholder.
///
/// A type placeholder reprents a value of which the exact type isn't
/// immediately known, and is to be inferred based on how the value is used.
/// Take this code for example:
///
///     let vals = []
///
/// While we know that `vals` is an array, we don't know the type of the values
/// in the array. In this case we use a type placeholder, meaning that `vals` is
/// of type `Array[V₁]` where V₁ is a type placeholder.
///
/// At some point we may push a value into the array, for example:
///
///     vals.push(42)
///
/// In this case V₁ is assigned to `Int`, and we end up with `vals` inferred as
/// `Array[Int]`.
///
/// The concept of type placeholder is taken from the Hindley-Milner type
/// system.
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
    ///
    /// TODO: remove
    depending: Vec<TypePlaceholderId>,

    /// The type parameter a type must be compatible with before it can be
    /// assigned to this type variable.
    required: Option<TypeParameterId>,
}

impl TypePlaceholder {
    fn alloc(
        db: &mut Database,
        required: Option<TypeParameterId>,
    ) -> TypePlaceholderId {
        let id = db.type_placeholders.len();
        let typ = TypePlaceholder {
            value: Cell::new(TypeRef::Unknown),
            depending: Vec::new(),
            required,
        };

        db.type_placeholders.push(typ);
        TypePlaceholderId(id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypePlaceholderId(pub(crate) usize);

impl TypePlaceholderId {
    pub fn value(self, db: &Database) -> Option<TypeRef> {
        match self.get(db).value.get() {
            TypeRef::Unknown => None,
            value => Some(value),
        }
    }

    // TODO: rename to value()
    fn resolve(self, db: &Database) -> TypeRef {
        // Chains of type variables are very rare in practise, but they _can_
        // occur and thus must be handled. Because they are so rare and unlikely
        // to be more than 2-3 levels deep, we just use recursion here instead
        // of a loop.
        let typ = self.get(db).value.get();

        match typ {
            TypeRef::Placeholder(id) => id.resolve(db),
            _ => typ,
        }
    }

    fn required(self, db: &Database) -> Option<TypeParameterId> {
        self.get(db).required
    }

    fn add_depending(self, db: &mut Database, placeholder: TypePlaceholderId) {
        self.get_mut(db).depending.push(placeholder);
    }

    fn assign(self, db: &Database, value: TypeRef) {
        // Assigning placeholders to themselves isn't useful and results in
        // resolve() getting stuck.
        if let TypeRef::Placeholder(id) = value {
            if id.0 == self.0 {
                return;
            }
        }

        self.get(db).value.set(value);

        // TODO: remove
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

    /// Type comparisons that have been performed and should be skipped when
    /// performed again.
    ///
    /// When type-checking recursive types we may end up comparing the same
    /// types over and over (e.g. "is A compatible with B[A]"). To handle this
    /// we cache comparisons here and simply treat the comparison as valid when
    /// encountering a cached comparison. As the outer-most/first occurrence of
    /// a comparison is still performed (we just short-circuit recursive
    /// checks), the type-checking results are still correct.
    ///
    /// If the outer-most case is invalid, it doesn't matter what result is
    /// produced for the inner/recursive checks, because the outer check would
    /// return `false` anyway.
    checked: HashSet<(TypeRef, TypeRef)>,
}

impl TypeContext {
    pub fn new(self_type_id: TypeId) -> Self {
        Self {
            self_type: self_type_id,
            type_arguments: TypeArguments::new(),
            depth: 0,
            checked: HashSet::new(),
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

        Self { self_type, type_arguments, depth: 0, checked: HashSet::new() }
    }

    pub fn with_arguments(
        self_type_id: TypeId,
        type_arguments: TypeArguments,
    ) -> Self {
        Self {
            self_type: self_type_id,
            type_arguments,
            depth: 0,
            checked: HashSet::new(),
        }
    }
}

/// A type parameter for a method or class.
pub struct TypeParameter {
    /// The name of the type parameter.
    name: String,

    /// The traits that must be implemented before a type can be assigned to
    /// this type parameter.
    requirements: Vec<TraitInstance>,

    /// If mutable references to this type parameter are allowed.
    mutable: bool,

    /// The ID of the original type parameter in case the current one is a
    /// parameter introduced through additional type bounds.
    original: Option<TypeParameterId>,
}

impl TypeParameter {
    pub fn alloc(db: &mut Database, name: String) -> TypeParameterId {
        let id = db.type_parameters.len();
        let typ = TypeParameter::new(name);

        db.type_parameters.push(typ);
        TypeParameterId(id)
    }

    fn new(name: String) -> Self {
        Self { name, requirements: Vec::new(), mutable: false, original: None }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TypeParameterId(pub usize);

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

    pub fn set_original(self, db: &mut Database, parameter: TypeParameterId) {
        self.get_mut(db).original = Some(parameter);
    }

    pub fn original(self, db: &Database) -> Option<TypeParameterId> {
        self.get(db).original
    }

    pub fn set_mutable(self, db: &mut Database) {
        self.get_mut(db).mutable = true;
    }

    pub fn is_mutable(self, db: &Database) -> bool {
        self.get(db).mutable
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
        rules: Rules,
    ) -> bool {
        match with {
            TypeId::TraitInstance(theirs) => {
                self.type_check_with_trait_instance(db, theirs, context, rules)
            }
            TypeId::TypeParameter(theirs) => {
                self.type_check_with_type_parameter(db, theirs, context, rules)
            }
            _ => false,
        }
    }

    fn type_check_with_type_parameter(
        self,
        db: &mut Database,
        with: TypeParameterId,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        if self == with {
            return true;
        }

        with.all_requirements_met(db, |db, req| {
            self.type_check_with_trait_instance(db, req, context, rules)
        })
    }

    fn type_check_with_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        self.get(db).requirements.clone().into_iter().any(|req| {
            req.type_check_with_trait_instance(db, instance, context, rules)
        })
    }

    fn as_rigid_type(self, bounds: &TypeBounds) -> TypeId {
        TypeId::RigidTypeParameter(bounds.get(self).unwrap_or(self))
    }

    fn as_owned_rigid(self) -> TypeRef {
        TypeRef::Owned(TypeId::RigidTypeParameter(self))
    }
}

/// Type parameters and the types assigned to them.
#[derive(Clone, Debug)]
pub struct TypeArguments {
    /// We use a HashMap as parameters can be assigned in any order, and some
    /// may not be assigned at all.
    mapping: HashMap<TypeParameterId, TypeRef>,
}

impl TypeArguments {
    pub fn for_class(db: &Database, instance: ClassInstance) -> TypeArguments {
        if instance.instance_of().is_generic(db) {
            instance.type_arguments(db).clone()
        } else {
            TypeArguments::new()
        }
    }

    // TODO: remove?
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

    // TODO: if `parameter` has `original` set, map to that instead.
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
        rules: Rules,
    ) -> bool {
        match with {
            TypeId::TraitInstance(ins) => {
                self.type_check_with_trait_instance(db, ins, context, rules)
            }
            TypeId::TypeParameter(id) => {
                id.all_requirements_met(db, |db, req| {
                    self.type_check_with_trait_instance(db, req, context, rules)
                })
            }
            _ => false,
        }
    }

    fn type_check_with_trait_instance(
        self,
        db: &mut Database,
        instance: TraitInstance,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        if self == instance {
            return true;
        }

        if self.instance_of != instance.instance_of {
            return if rules.subtyping {
                self.instance_of
                    .get(db)
                    .required_traits
                    .clone()
                    .into_iter()
                    .any(|req| {
                        req.type_check_with_trait_instance(
                            db, instance, context, rules,
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

        our_trait.type_parameters.values().clone().into_iter().all(|param| {
            our_args
                .get(param)
                .zip(their_args.get(param))
                .map(|(ours, theirs)| {
                    ours.type_check(db, theirs, context, rules)
                })
                .unwrap_or(false)
        })
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
        let rules = Rules::new();

        self.instance_of.get(db).required_traits.clone().into_iter().any(
            |req| {
                req.type_check_with_trait_instance(db, instance, context, rules)
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
/// This structure maps the original type parameters (`T` in this case) to type
/// parameters created for the bounds. These new type parameters have their
/// requirements set to the union of the original type parameter's requirements,
/// and the requirements specified in the bounds. In other words, if the
/// original parameter is defined as `T: A` and the bounds specify `T: B`, this
/// structure maps `T: A` to `T: A + B`.
#[derive(Clone, Debug)]
pub struct TypeBounds {
    mapping: HashMap<TypeParameterId, TypeParameterId>,
}

impl TypeBounds {
    pub fn new() -> Self {
        Self { mapping: HashMap::default() }
    }

    // TODO: handle bounded parameters
    pub fn set(&mut self, parameter: TypeParameterId, bounds: TypeParameterId) {
        self.mapping.insert(parameter, bounds);
    }

    pub fn get(&self, parameter: TypeParameterId) -> Option<TypeParameterId> {
        self.mapping.get(&parameter).cloned()
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&TypeParameterId, &TypeParameterId)> {
        self.mapping.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    pub fn union(&self, with: &TypeBounds) -> TypeBounds {
        let mut union = self.clone();

        for (&key, &val) in &with.mapping {
            union.set(key, val);
        }

        union
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
    Closure,
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

    pub fn is_closure(self) -> bool {
        matches!(self, ClassKind::Closure)
    }
}

/// An Inko class as declared using the `class` keyword.
pub struct Class {
    kind: ClassKind,
    name: String,
    atomic: bool,
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
            atomic: kind.is_async(),
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

    fn atomic(name: String) -> Self {
        let mut class = Self::new(
            name,
            ClassKind::Regular,
            Visibility::Public,
            ModuleId(DEFAULT_BUILTIN_MODULE_ID),
        );

        class.atomic = true;
        class
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

    pub fn channel() -> ClassId {
        ClassId(CHANNEL_ID)
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

    pub fn is_atomic(self, db: &Database) -> bool {
        self.get(db).atomic
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

    pub fn is_builtin(self) -> bool {
        self.0 <= CHANNEL_ID
    }

    fn get(self, db: &Database) -> &Class {
        &db.classes[self.0 as usize]
    }

    fn get_mut(self, db: &mut Database) -> &mut Class {
        &mut db.classes[self.0 as usize]
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

    pub fn with_types(
        db: &mut Database,
        class: ClassId,
        arguments: Vec<TypeRef>,
    ) -> Self {
        let mut args = TypeArguments::new();

        for (index, param) in class.type_parameters(db).into_iter().enumerate()
        {
            let val = arguments.get(index).cloned().unwrap_or_else(|| {
                TypeRef::Placeholder(TypePlaceholder::alloc(db, Some(param)))
            });

            args.assign(param, val);
        }

        Self::generic(db, class, args)
    }

    pub fn empty(db: &mut Database, class: ClassId) -> Self {
        if !class.is_generic(db) {
            return Self::new(class);
        }

        let mut args = TypeArguments::new();

        for param in class.type_parameters(db) {
            args.assign(param, TypeRef::placeholder(db, Some(param)));
        }

        Self::generic(db, class, args)
    }

    pub fn instance_of(self) -> ClassId {
        self.instance_of
    }

    pub fn type_arguments(self, db: &Database) -> &TypeArguments {
        &db.type_arguments[self.type_arguments as usize]
    }

    pub fn first_type_argument(self, db: &Database) -> Option<TypeRef> {
        db.type_arguments[self.type_arguments as usize]
            .mapping
            .values()
            .cloned()
            .next()
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
        rules: Rules,
    ) -> bool {
        if !rules.subtyping {
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

        let trait_instance = trait_impl.instance;

        // When comparing X with X itself we can just bypass all of the below.
        if trait_instance == instance && trait_impl.bounds.mapping.is_empty() {
            return true;
        }

        if self.instance_of.is_generic(db) {
            // The generic trait implementation may refer to (or contain a type
            // that refers) to a type parameter defined in our class. If we end
            // up comparing such a type parameter, we must compare its assigned
            // value instead if there is any.
            for (&param, &val) in &self.type_arguments(db).mapping {
                if context.type_arguments.get(param).is_none() {
                    context.type_arguments.assign(param, val);
                }
            }

            for (param, bound) in trait_impl.bounds.mapping.into_iter() {
                if let Some(val) = context.type_arguments.get(param) {
                    // The trait implementation may refer to the bounded
                    // parameters. Because these are copies of the original
                    // parameters along with the additional requirements, if any
                    // of the original parameters are assigned a value we must
                    // also create an entry for the bounded parameter.
                    context.type_arguments.assign(bound, val);

                    if !val.is_compatible_with_type_parameter(
                        db, bound, context, rules,
                    ) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        trait_instance
            .type_check_with_trait_instance(db, instance, context, rules)
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
        rules: Rules,
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
                                ours.type_check(db, theirs, context, rules)
                            })
                            .unwrap_or(false)
                    },
                )
            }
            TypeId::TraitInstance(ins) => {
                self.type_check_with_trait_instance(db, ins, context, rules)
            }
            TypeId::TypeParameter(id) => {
                id.all_requirements_met(db, |db, req| {
                    self.type_check_with_trait_instance(db, req, context, rules)
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

    // TODO: remove
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

        let rules = Rules::none();

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
                rules,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum BuiltinFunction {
    ArrayCapacity,
    ArrayClear,
    ArrayDrop,
    ArrayGet,
    ArrayLength,
    ArrayPop,
    ArrayPush,
    ArrayRemove,
    ArrayReserve,
    ArraySet,
    ByteArrayNew,
    ByteArrayAppend,
    ByteArrayClear,
    ByteArrayClone,
    ByteArrayCopyFrom,
    ByteArrayDrainToString,
    ByteArrayDrop,
    ByteArrayEq,
    ByteArrayGet,
    ByteArrayLength,
    ByteArrayPop,
    ByteArrayPush,
    ByteArrayRemove,
    ByteArrayResize,
    ByteArraySet,
    ByteArraySlice,
    ByteArrayToString,
    ChildProcessDrop,
    ChildProcessSpawn,
    ChildProcessStderrClose,
    ChildProcessStderrRead,
    ChildProcessStdinClose,
    ChildProcessStdinFlush,
    ChildProcessStdinWriteBytes,
    ChildProcessStdinWriteString,
    ChildProcessStdoutClose,
    ChildProcessStdoutRead,
    ChildProcessTryWait,
    ChildProcessWait,
    CpuCores,
    DirectoryCreate,
    DirectoryCreateRecursive,
    DirectoryList,
    DirectoryRemove,
    DirectoryRemoveRecursive,
    EnvArguments,
    EnvExecutable,
    EnvGet,
    EnvGetWorkingDirectory,
    EnvHomeDirectory,
    EnvPlatform,
    EnvSetWorkingDirectory,
    EnvTempDirectory,
    EnvVariables,
    Exit,
    FileCopy,
    FileDrop,
    FileFlush,
    FileOpen,
    FileRead,
    FileRemove,
    FileSeek,
    FileSize,
    FileWriteBytes,
    FileWriteString,
    FloatAdd,
    FloatCeil,
    FloatDiv,
    FloatEq,
    FloatFloor,
    FloatFromBits,
    FloatGe,
    FloatGt,
    FloatIsInf,
    FloatIsNan,
    FloatLe,
    FloatLt,
    FloatMod,
    FloatMul,
    FloatRound,
    FloatSub,
    FloatToBits,
    FloatToInt,
    FloatToString,
    ChannelDrop,
    ChannelNew,
    ChannelReceive,
    ChannelReceiveUntil,
    ChannelSend,
    ChannelTryReceive,
    ChannelWait,
    GetNil,
    HasherDrop,
    HasherNew,
    HasherToHash,
    HasherWriteInt,
    IntAdd,
    IntBitAnd,
    IntBitNot,
    IntBitOr,
    IntBitXor,
    IntDiv,
    IntEq,
    IntGe,
    IntGt,
    IntLe,
    IntLt,
    IntRem,
    IntMul,
    IntPow,
    IntRotateLeft,
    IntRotateRight,
    IntShl,
    IntShr,
    IntSub,
    IntToFloat,
    IntToString,
    IntUnsignedShr,
    IntWrappingAdd,
    IntWrappingMul,
    IntWrappingSub,
    IsNull,
    Moved,
    ObjectEq,
    Panic,
    PanicThrown,
    PathAccessedAt,
    PathCreatedAt,
    PathExists,
    PathIsDirectory,
    PathIsFile,
    PathModifiedAt,
    ProcessStackFrameLine,
    ProcessStackFrameName,
    ProcessStackFramePath,
    ProcessStacktrace,
    ProcessStacktraceDrop,
    ProcessStacktraceLength,
    ProcessSuspend,
    RandomBytes,
    RandomDrop,
    RandomFloat,
    RandomFloatRange,
    RandomFromInt,
    RandomInt,
    RandomIntRange,
    RandomNew,
    SocketAccept,
    SocketAddressPairAddress,
    SocketAddressPairDrop,
    SocketAddressPairPort,
    SocketNew,
    SocketBind,
    SocketConnect,
    SocketDrop,
    SocketListen,
    SocketLocalAddress,
    SocketPeerAddress,
    SocketRead,
    SocketReceiveFrom,
    SocketSendBytesTo,
    SocketSendStringTo,
    SocketSetBroadcast,
    SocketSetKeepalive,
    SocketSetLinger,
    SocketSetNodelay,
    SocketSetOnlyV6,
    SocketSetRecvSize,
    SocketSetReuseAddress,
    SocketSetReusePort,
    SocketSetSendSize,
    SocketSetTtl,
    SocketShutdownRead,
    SocketShutdownReadWrite,
    SocketShutdownWrite,
    SocketTryClone,
    SocketWriteBytes,
    SocketWriteString,
    StderrFlush,
    StderrWriteBytes,
    StderrWriteString,
    StdinRead,
    StdoutFlush,
    StdoutWriteBytes,
    StdoutWriteString,
    StringByte,
    StringCharacters,
    StringCharactersDrop,
    StringCharactersNext,
    StringConcat,
    StringConcatArray,
    StringDrop,
    StringEq,
    StringSize,
    StringSliceBytes,
    StringToByteArray,
    StringToFloat,
    StringToInt,
    StringToLower,
    StringToUpper,
    TimeMonotonic,
    TimeSystem,
    TimeSystemOffset,
}

impl BuiltinFunction {
    pub fn mapping() -> HashMap<String, Self> {
        let mut map = HashMap::new();
        let funcs = vec![
            BuiltinFunction::ArrayCapacity,
            BuiltinFunction::ArrayClear,
            BuiltinFunction::ArrayDrop,
            BuiltinFunction::ArrayGet,
            BuiltinFunction::ArrayLength,
            BuiltinFunction::ArrayPop,
            BuiltinFunction::ArrayPush,
            BuiltinFunction::ArrayRemove,
            BuiltinFunction::ArrayReserve,
            BuiltinFunction::ArraySet,
            BuiltinFunction::ByteArrayNew,
            BuiltinFunction::ByteArrayAppend,
            BuiltinFunction::ByteArrayClear,
            BuiltinFunction::ByteArrayClone,
            BuiltinFunction::ByteArrayCopyFrom,
            BuiltinFunction::ByteArrayDrainToString,
            BuiltinFunction::ByteArrayDrop,
            BuiltinFunction::ByteArrayEq,
            BuiltinFunction::ByteArrayGet,
            BuiltinFunction::ByteArrayLength,
            BuiltinFunction::ByteArrayPop,
            BuiltinFunction::ByteArrayPush,
            BuiltinFunction::ByteArrayRemove,
            BuiltinFunction::ByteArrayResize,
            BuiltinFunction::ByteArraySet,
            BuiltinFunction::ByteArraySlice,
            BuiltinFunction::ByteArrayToString,
            BuiltinFunction::ChildProcessDrop,
            BuiltinFunction::ChildProcessSpawn,
            BuiltinFunction::ChildProcessStderrClose,
            BuiltinFunction::ChildProcessStderrRead,
            BuiltinFunction::ChildProcessStdinClose,
            BuiltinFunction::ChildProcessStdinFlush,
            BuiltinFunction::ChildProcessStdinWriteBytes,
            BuiltinFunction::ChildProcessStdinWriteString,
            BuiltinFunction::ChildProcessStdoutClose,
            BuiltinFunction::ChildProcessStdoutRead,
            BuiltinFunction::ChildProcessTryWait,
            BuiltinFunction::ChildProcessWait,
            BuiltinFunction::CpuCores,
            BuiltinFunction::DirectoryCreate,
            BuiltinFunction::DirectoryCreateRecursive,
            BuiltinFunction::DirectoryList,
            BuiltinFunction::DirectoryRemove,
            BuiltinFunction::DirectoryRemoveRecursive,
            BuiltinFunction::EnvArguments,
            BuiltinFunction::EnvExecutable,
            BuiltinFunction::EnvGet,
            BuiltinFunction::EnvGetWorkingDirectory,
            BuiltinFunction::EnvHomeDirectory,
            BuiltinFunction::EnvPlatform,
            BuiltinFunction::EnvSetWorkingDirectory,
            BuiltinFunction::EnvTempDirectory,
            BuiltinFunction::EnvVariables,
            BuiltinFunction::Exit,
            BuiltinFunction::FileCopy,
            BuiltinFunction::FileDrop,
            BuiltinFunction::FileFlush,
            BuiltinFunction::FileOpen,
            BuiltinFunction::FileRead,
            BuiltinFunction::FileRemove,
            BuiltinFunction::FileSeek,
            BuiltinFunction::FileSize,
            BuiltinFunction::FileWriteBytes,
            BuiltinFunction::FileWriteString,
            BuiltinFunction::FloatAdd,
            BuiltinFunction::FloatCeil,
            BuiltinFunction::FloatDiv,
            BuiltinFunction::FloatEq,
            BuiltinFunction::FloatFloor,
            BuiltinFunction::FloatFromBits,
            BuiltinFunction::FloatGe,
            BuiltinFunction::FloatGt,
            BuiltinFunction::FloatIsInf,
            BuiltinFunction::FloatIsNan,
            BuiltinFunction::FloatLe,
            BuiltinFunction::FloatLt,
            BuiltinFunction::FloatMod,
            BuiltinFunction::FloatMul,
            BuiltinFunction::FloatRound,
            BuiltinFunction::FloatSub,
            BuiltinFunction::FloatToBits,
            BuiltinFunction::FloatToInt,
            BuiltinFunction::FloatToString,
            BuiltinFunction::ChannelDrop,
            BuiltinFunction::ChannelNew,
            BuiltinFunction::ChannelReceive,
            BuiltinFunction::ChannelReceiveUntil,
            BuiltinFunction::ChannelSend,
            BuiltinFunction::ChannelTryReceive,
            BuiltinFunction::ChannelWait,
            BuiltinFunction::GetNil,
            BuiltinFunction::HasherDrop,
            BuiltinFunction::HasherNew,
            BuiltinFunction::HasherToHash,
            BuiltinFunction::HasherWriteInt,
            BuiltinFunction::IntAdd,
            BuiltinFunction::IntBitAnd,
            BuiltinFunction::IntBitNot,
            BuiltinFunction::IntBitOr,
            BuiltinFunction::IntBitXor,
            BuiltinFunction::IntDiv,
            BuiltinFunction::IntEq,
            BuiltinFunction::IntGe,
            BuiltinFunction::IntGt,
            BuiltinFunction::IntLe,
            BuiltinFunction::IntLt,
            BuiltinFunction::IntRem,
            BuiltinFunction::IntMul,
            BuiltinFunction::IntPow,
            BuiltinFunction::IntRotateLeft,
            BuiltinFunction::IntRotateRight,
            BuiltinFunction::IntShl,
            BuiltinFunction::IntShr,
            BuiltinFunction::IntSub,
            BuiltinFunction::IntToFloat,
            BuiltinFunction::IntToString,
            BuiltinFunction::IntUnsignedShr,
            BuiltinFunction::IntWrappingAdd,
            BuiltinFunction::IntWrappingMul,
            BuiltinFunction::IntWrappingSub,
            BuiltinFunction::IsNull,
            BuiltinFunction::Moved,
            BuiltinFunction::ObjectEq,
            BuiltinFunction::Panic,
            BuiltinFunction::PanicThrown,
            BuiltinFunction::PathAccessedAt,
            BuiltinFunction::PathCreatedAt,
            BuiltinFunction::PathExists,
            BuiltinFunction::PathIsDirectory,
            BuiltinFunction::PathIsFile,
            BuiltinFunction::PathModifiedAt,
            BuiltinFunction::ProcessStackFrameLine,
            BuiltinFunction::ProcessStackFrameName,
            BuiltinFunction::ProcessStackFramePath,
            BuiltinFunction::ProcessStacktrace,
            BuiltinFunction::ProcessStacktraceDrop,
            BuiltinFunction::ProcessStacktraceLength,
            BuiltinFunction::ProcessSuspend,
            BuiltinFunction::RandomBytes,
            BuiltinFunction::RandomDrop,
            BuiltinFunction::RandomFloat,
            BuiltinFunction::RandomFloatRange,
            BuiltinFunction::RandomFromInt,
            BuiltinFunction::RandomInt,
            BuiltinFunction::RandomIntRange,
            BuiltinFunction::RandomNew,
            BuiltinFunction::SocketAccept,
            BuiltinFunction::SocketAddressPairAddress,
            BuiltinFunction::SocketAddressPairDrop,
            BuiltinFunction::SocketAddressPairPort,
            BuiltinFunction::SocketNew,
            BuiltinFunction::SocketBind,
            BuiltinFunction::SocketConnect,
            BuiltinFunction::SocketDrop,
            BuiltinFunction::SocketListen,
            BuiltinFunction::SocketLocalAddress,
            BuiltinFunction::SocketPeerAddress,
            BuiltinFunction::SocketRead,
            BuiltinFunction::SocketReceiveFrom,
            BuiltinFunction::SocketSendBytesTo,
            BuiltinFunction::SocketSendStringTo,
            BuiltinFunction::SocketSetBroadcast,
            BuiltinFunction::SocketSetKeepalive,
            BuiltinFunction::SocketSetLinger,
            BuiltinFunction::SocketSetNodelay,
            BuiltinFunction::SocketSetOnlyV6,
            BuiltinFunction::SocketSetRecvSize,
            BuiltinFunction::SocketSetReuseAddress,
            BuiltinFunction::SocketSetReusePort,
            BuiltinFunction::SocketSetSendSize,
            BuiltinFunction::SocketSetTtl,
            BuiltinFunction::SocketShutdownRead,
            BuiltinFunction::SocketShutdownReadWrite,
            BuiltinFunction::SocketShutdownWrite,
            BuiltinFunction::SocketTryClone,
            BuiltinFunction::SocketWriteBytes,
            BuiltinFunction::SocketWriteString,
            BuiltinFunction::StderrFlush,
            BuiltinFunction::StderrWriteBytes,
            BuiltinFunction::StderrWriteString,
            BuiltinFunction::StdinRead,
            BuiltinFunction::StdoutFlush,
            BuiltinFunction::StdoutWriteBytes,
            BuiltinFunction::StdoutWriteString,
            BuiltinFunction::StringByte,
            BuiltinFunction::StringCharacters,
            BuiltinFunction::StringCharactersDrop,
            BuiltinFunction::StringCharactersNext,
            BuiltinFunction::StringConcat,
            BuiltinFunction::StringConcatArray,
            BuiltinFunction::StringDrop,
            BuiltinFunction::StringEq,
            BuiltinFunction::StringSize,
            BuiltinFunction::StringSliceBytes,
            BuiltinFunction::StringToByteArray,
            BuiltinFunction::StringToFloat,
            BuiltinFunction::StringToInt,
            BuiltinFunction::StringToLower,
            BuiltinFunction::StringToUpper,
            BuiltinFunction::TimeMonotonic,
            BuiltinFunction::TimeSystem,
            BuiltinFunction::TimeSystemOffset,
        ];

        for func in funcs {
            map.insert(func.name().to_string(), func);
        }

        map
    }

    pub fn name(self) -> &'static str {
        match self {
            BuiltinFunction::ArrayCapacity => "array_capacity",
            BuiltinFunction::ArrayClear => "array_clear",
            BuiltinFunction::ArrayDrop => "array_drop",
            BuiltinFunction::ArrayGet => "array_get",
            BuiltinFunction::ArrayLength => "array_length",
            BuiltinFunction::ArrayPop => "array_pop",
            BuiltinFunction::ArrayPush => "array_push",
            BuiltinFunction::ArrayRemove => "array_remove",
            BuiltinFunction::ArrayReserve => "array_reserve",
            BuiltinFunction::ArraySet => "array_set",
            BuiltinFunction::ByteArrayNew => "byte_array_new",
            BuiltinFunction::ByteArrayAppend => "byte_array_append",
            BuiltinFunction::ByteArrayClear => "byte_array_clear",
            BuiltinFunction::ByteArrayClone => "byte_array_clone",
            BuiltinFunction::ByteArrayCopyFrom => "byte_array_copy_from",
            BuiltinFunction::ByteArrayDrainToString => {
                "byte_array_drain_to_string"
            }
            BuiltinFunction::ByteArrayDrop => "byte_array_drop",
            BuiltinFunction::ByteArrayEq => "byte_array_eq",
            BuiltinFunction::ByteArrayGet => "byte_array_get",
            BuiltinFunction::ByteArrayLength => "byte_array_length",
            BuiltinFunction::ByteArrayPop => "byte_array_pop",
            BuiltinFunction::ByteArrayPush => "byte_array_push",
            BuiltinFunction::ByteArrayRemove => "byte_array_remove",
            BuiltinFunction::ByteArrayResize => "byte_array_resize",
            BuiltinFunction::ByteArraySet => "byte_array_set",
            BuiltinFunction::ByteArraySlice => "byte_array_slice",
            BuiltinFunction::ByteArrayToString => "byte_array_to_string",
            BuiltinFunction::ChildProcessDrop => "child_process_drop",
            BuiltinFunction::ChildProcessSpawn => "child_process_spawn",
            BuiltinFunction::ChildProcessStderrClose => {
                "child_process_stderr_close"
            }
            BuiltinFunction::ChildProcessStderrRead => {
                "child_process_stderr_read"
            }
            BuiltinFunction::ChildProcessStdinClose => {
                "child_process_stdin_close"
            }
            BuiltinFunction::ChildProcessStdinFlush => {
                "child_process_stdin_flush"
            }
            BuiltinFunction::ChildProcessStdinWriteBytes => {
                "child_process_stdin_write_bytes"
            }
            BuiltinFunction::ChildProcessStdinWriteString => {
                "child_process_stdin_write_string"
            }
            BuiltinFunction::ChildProcessStdoutClose => {
                "child_process_stdout_close"
            }
            BuiltinFunction::ChildProcessStdoutRead => {
                "child_process_stdout_read"
            }
            BuiltinFunction::ChildProcessTryWait => "child_process_try_wait",
            BuiltinFunction::ChildProcessWait => "child_process_wait",
            BuiltinFunction::CpuCores => "cpu_cores",
            BuiltinFunction::DirectoryCreate => "directory_create",
            BuiltinFunction::DirectoryCreateRecursive => {
                "directory_create_recursive"
            }
            BuiltinFunction::DirectoryList => "directory_list",
            BuiltinFunction::DirectoryRemove => "directory_remove",
            BuiltinFunction::DirectoryRemoveRecursive => {
                "directory_remove_recursive"
            }
            BuiltinFunction::EnvArguments => "env_arguments",
            BuiltinFunction::EnvExecutable => "env_executable",
            BuiltinFunction::EnvGet => "env_get",
            BuiltinFunction::EnvGetWorkingDirectory => {
                "env_get_working_directory"
            }
            BuiltinFunction::EnvHomeDirectory => "env_home_directory",
            BuiltinFunction::EnvPlatform => "env_platform",
            BuiltinFunction::EnvSetWorkingDirectory => {
                "env_set_working_directory"
            }
            BuiltinFunction::EnvTempDirectory => "env_temp_directory",
            BuiltinFunction::EnvVariables => "env_variables",
            BuiltinFunction::Exit => "exit",
            BuiltinFunction::FileCopy => "file_copy",
            BuiltinFunction::FileDrop => "file_drop",
            BuiltinFunction::FileFlush => "file_flush",
            BuiltinFunction::FileOpen => "file_open",
            BuiltinFunction::FileRead => "file_read",
            BuiltinFunction::FileRemove => "file_remove",
            BuiltinFunction::FileSeek => "file_seek",
            BuiltinFunction::FileSize => "file_size",
            BuiltinFunction::FileWriteBytes => "file_write_bytes",
            BuiltinFunction::FileWriteString => "file_write_string",
            BuiltinFunction::FloatAdd => "float_add",
            BuiltinFunction::FloatCeil => "float_ceil",
            BuiltinFunction::FloatDiv => "float_div",
            BuiltinFunction::FloatEq => "float_eq",
            BuiltinFunction::FloatFloor => "float_floor",
            BuiltinFunction::FloatFromBits => "float_from_bits",
            BuiltinFunction::FloatGe => "float_ge",
            BuiltinFunction::FloatGt => "float_gt",
            BuiltinFunction::FloatIsInf => "float_is_inf",
            BuiltinFunction::FloatIsNan => "float_is_nan",
            BuiltinFunction::FloatLe => "float_le",
            BuiltinFunction::FloatLt => "float_lt",
            BuiltinFunction::FloatMod => "float_mod",
            BuiltinFunction::FloatMul => "float_mul",
            BuiltinFunction::FloatRound => "float_round",
            BuiltinFunction::FloatSub => "float_sub",
            BuiltinFunction::FloatToBits => "float_to_bits",
            BuiltinFunction::FloatToInt => "float_to_int",
            BuiltinFunction::FloatToString => "float_to_string",
            BuiltinFunction::ChannelReceive => "channel_receive",
            BuiltinFunction::ChannelReceiveUntil => "channel_receive_until",
            BuiltinFunction::ChannelDrop => "channel_drop",
            BuiltinFunction::ChannelWait => "channel_wait",
            BuiltinFunction::ChannelNew => "channel_new",
            BuiltinFunction::ChannelSend => "channel_send",
            BuiltinFunction::ChannelTryReceive => "channel_try_receive",
            BuiltinFunction::GetNil => "get_nil",
            BuiltinFunction::HasherDrop => "hasher_drop",
            BuiltinFunction::HasherNew => "hasher_new",
            BuiltinFunction::HasherToHash => "hasher_to_hash",
            BuiltinFunction::HasherWriteInt => "hasher_write_int",
            BuiltinFunction::IntAdd => "int_add",
            BuiltinFunction::IntBitAnd => "int_bit_and",
            BuiltinFunction::IntBitNot => "int_bit_not",
            BuiltinFunction::IntBitOr => "int_bit_or",
            BuiltinFunction::IntBitXor => "int_bit_xor",
            BuiltinFunction::IntDiv => "int_div",
            BuiltinFunction::IntEq => "int_eq",
            BuiltinFunction::IntGe => "int_ge",
            BuiltinFunction::IntGt => "int_gt",
            BuiltinFunction::IntLe => "int_le",
            BuiltinFunction::IntLt => "int_lt",
            BuiltinFunction::IntRem => "int_rem",
            BuiltinFunction::IntMul => "int_mul",
            BuiltinFunction::IntPow => "int_pow",
            BuiltinFunction::IntRotateLeft => "int_rotate_left",
            BuiltinFunction::IntRotateRight => "int_rotate_right",
            BuiltinFunction::IntShl => "int_shl",
            BuiltinFunction::IntShr => "int_shr",
            BuiltinFunction::IntSub => "int_sub",
            BuiltinFunction::IntToFloat => "int_to_float",
            BuiltinFunction::IntToString => "int_to_string",
            BuiltinFunction::IntUnsignedShr => "int_unsigned_shr",
            BuiltinFunction::IntWrappingAdd => "int_wrapping_add",
            BuiltinFunction::IntWrappingMul => "int_wrapping_mul",
            BuiltinFunction::IntWrappingSub => "int_wrapping_sub",
            BuiltinFunction::IsNull => "is_null",
            BuiltinFunction::Moved => "moved",
            BuiltinFunction::ObjectEq => "object_eq",
            BuiltinFunction::Panic => "panic",
            BuiltinFunction::PanicThrown => "panic_thrown",
            BuiltinFunction::PathAccessedAt => "path_accessed_at",
            BuiltinFunction::PathCreatedAt => "path_created_at",
            BuiltinFunction::PathExists => "path_exists",
            BuiltinFunction::PathIsDirectory => "path_is_directory",
            BuiltinFunction::PathIsFile => "path_is_file",
            BuiltinFunction::PathModifiedAt => "path_modified_at",
            BuiltinFunction::ProcessStackFrameLine => {
                "process_stack_frame_line"
            }
            BuiltinFunction::ProcessStackFrameName => {
                "process_stack_frame_name"
            }
            BuiltinFunction::ProcessStackFramePath => {
                "process_stack_frame_path"
            }
            BuiltinFunction::ProcessStacktrace => "process_stacktrace",
            BuiltinFunction::ProcessStacktraceDrop => "process_stacktrace_drop",
            BuiltinFunction::ProcessStacktraceLength => {
                "process_stacktrace_length"
            }
            BuiltinFunction::ProcessSuspend => "process_suspend",
            BuiltinFunction::RandomBytes => "random_bytes",
            BuiltinFunction::RandomDrop => "random_drop",
            BuiltinFunction::RandomFloat => "random_float",
            BuiltinFunction::RandomFloatRange => "random_float_range",
            BuiltinFunction::RandomFromInt => "random_from_int",
            BuiltinFunction::RandomInt => "random_int",
            BuiltinFunction::RandomIntRange => "random_int_range",
            BuiltinFunction::RandomNew => "random_new",
            BuiltinFunction::SocketAccept => "socket_accept",
            BuiltinFunction::SocketAddressPairAddress => {
                "socket_address_pair_address"
            }
            BuiltinFunction::SocketAddressPairDrop => {
                "socket_address_pair_drop"
            }
            BuiltinFunction::SocketAddressPairPort => {
                "socket_address_pair_port"
            }
            BuiltinFunction::SocketNew => "socket_new",
            BuiltinFunction::SocketBind => "socket_bind",
            BuiltinFunction::SocketConnect => "socket_connect",
            BuiltinFunction::SocketDrop => "socket_drop",
            BuiltinFunction::SocketListen => "socket_listen",
            BuiltinFunction::SocketLocalAddress => "socket_local_address",
            BuiltinFunction::SocketPeerAddress => "socket_peer_address",
            BuiltinFunction::SocketRead => "socket_read",
            BuiltinFunction::SocketReceiveFrom => "socket_receive_from",
            BuiltinFunction::SocketSendBytesTo => "socket_send_bytes_to",
            BuiltinFunction::SocketSendStringTo => "socket_send_string_to",
            BuiltinFunction::SocketSetBroadcast => "socket_set_broadcast",
            BuiltinFunction::SocketSetKeepalive => "socket_set_keepalive",
            BuiltinFunction::SocketSetLinger => "socket_set_linger",
            BuiltinFunction::SocketSetNodelay => "socket_set_nodelay",
            BuiltinFunction::SocketSetOnlyV6 => "socket_set_only_v6",
            BuiltinFunction::SocketSetRecvSize => "socket_set_recv_size",
            BuiltinFunction::SocketSetReuseAddress => {
                "socket_set_reuse_address"
            }
            BuiltinFunction::SocketSetReusePort => "socket_set_reuse_port",
            BuiltinFunction::SocketSetSendSize => "socket_set_send_size",
            BuiltinFunction::SocketSetTtl => "socket_set_ttl",
            BuiltinFunction::SocketShutdownRead => "socket_shutdown_read",
            BuiltinFunction::SocketShutdownReadWrite => {
                "socket_shutdown_read_write"
            }
            BuiltinFunction::SocketShutdownWrite => "socket_shutdown_write",
            BuiltinFunction::SocketTryClone => "socket_try_clone",
            BuiltinFunction::SocketWriteBytes => "socket_write_bytes",
            BuiltinFunction::SocketWriteString => "socket_write_string",
            BuiltinFunction::StderrFlush => "stderr_flush",
            BuiltinFunction::StderrWriteBytes => "stderr_write_bytes",
            BuiltinFunction::StderrWriteString => "stderr_write_string",
            BuiltinFunction::StdinRead => "stdin_read",
            BuiltinFunction::StdoutFlush => "stdout_flush",
            BuiltinFunction::StdoutWriteBytes => "stdout_write_bytes",
            BuiltinFunction::StdoutWriteString => "stdout_write_string",
            BuiltinFunction::StringByte => "string_byte",
            BuiltinFunction::StringCharacters => "string_characters",
            BuiltinFunction::StringCharactersDrop => "string_characters_drop",
            BuiltinFunction::StringCharactersNext => "string_characters_next",
            BuiltinFunction::StringConcat => "string_concat",
            BuiltinFunction::StringConcatArray => "string_concat_array",
            BuiltinFunction::StringDrop => "string_drop",
            BuiltinFunction::StringEq => "string_eq",
            BuiltinFunction::StringSize => "string_size",
            BuiltinFunction::StringSliceBytes => "string_slice_bytes",
            BuiltinFunction::StringToByteArray => "string_to_byte_array",
            BuiltinFunction::StringToFloat => "string_to_float",
            BuiltinFunction::StringToInt => "string_to_int",
            BuiltinFunction::StringToLower => "string_to_lower",
            BuiltinFunction::StringToUpper => "string_to_upper",
            BuiltinFunction::TimeMonotonic => "time_monotonic",
            BuiltinFunction::TimeSystem => "time_system",
            BuiltinFunction::TimeSystemOffset => "time_system_offset",
        }
    }

    pub fn return_type(self) -> TypeRef {
        match self {
            BuiltinFunction::ArrayCapacity => TypeRef::int(),
            BuiltinFunction::ArrayClear => TypeRef::nil(),
            BuiltinFunction::ArrayDrop => TypeRef::nil(),
            BuiltinFunction::ArrayGet => TypeRef::Any,
            BuiltinFunction::ArrayLength => TypeRef::int(),
            BuiltinFunction::ArrayPop => TypeRef::Any,
            BuiltinFunction::ArrayPush => TypeRef::nil(),
            BuiltinFunction::ArrayRemove => TypeRef::Any,
            BuiltinFunction::ArrayReserve => TypeRef::nil(),
            BuiltinFunction::ArraySet => TypeRef::Any,
            BuiltinFunction::ByteArrayNew => TypeRef::byte_array(),
            BuiltinFunction::ByteArrayAppend => TypeRef::nil(),
            BuiltinFunction::ByteArrayClear => TypeRef::nil(),
            BuiltinFunction::ByteArrayClone => TypeRef::byte_array(),
            BuiltinFunction::ByteArrayCopyFrom => TypeRef::int(),
            BuiltinFunction::ByteArrayDrainToString => TypeRef::string(),
            BuiltinFunction::ByteArrayDrop => TypeRef::nil(),
            BuiltinFunction::ByteArrayEq => TypeRef::boolean(),
            BuiltinFunction::ByteArrayGet => TypeRef::int(),
            BuiltinFunction::ByteArrayLength => TypeRef::int(),
            BuiltinFunction::ByteArrayPop => TypeRef::int(),
            BuiltinFunction::ByteArrayPush => TypeRef::nil(),
            BuiltinFunction::ByteArrayRemove => TypeRef::int(),
            BuiltinFunction::ByteArrayResize => TypeRef::nil(),
            BuiltinFunction::ByteArraySet => TypeRef::int(),
            BuiltinFunction::ByteArraySlice => TypeRef::byte_array(),
            BuiltinFunction::ByteArrayToString => TypeRef::string(),
            BuiltinFunction::ChildProcessDrop => TypeRef::Any,
            BuiltinFunction::ChildProcessSpawn => TypeRef::Any,
            BuiltinFunction::ChildProcessStderrClose => TypeRef::nil(),
            BuiltinFunction::ChildProcessStderrRead => TypeRef::int(),
            BuiltinFunction::ChildProcessStdinClose => TypeRef::nil(),
            BuiltinFunction::ChildProcessStdinFlush => TypeRef::nil(),
            BuiltinFunction::ChildProcessStdinWriteBytes => TypeRef::int(),
            BuiltinFunction::ChildProcessStdinWriteString => TypeRef::int(),
            BuiltinFunction::ChildProcessStdoutClose => TypeRef::nil(),
            BuiltinFunction::ChildProcessStdoutRead => TypeRef::int(),
            BuiltinFunction::ChildProcessTryWait => TypeRef::int(),
            BuiltinFunction::ChildProcessWait => TypeRef::int(),
            BuiltinFunction::CpuCores => TypeRef::int(),
            BuiltinFunction::DirectoryCreate => TypeRef::nil(),
            BuiltinFunction::DirectoryCreateRecursive => TypeRef::nil(),
            BuiltinFunction::DirectoryList => TypeRef::Any,
            BuiltinFunction::DirectoryRemove => TypeRef::nil(),
            BuiltinFunction::DirectoryRemoveRecursive => TypeRef::nil(),
            BuiltinFunction::EnvArguments => TypeRef::Any,
            BuiltinFunction::EnvExecutable => TypeRef::string(),
            BuiltinFunction::EnvGet => TypeRef::string(),
            BuiltinFunction::EnvGetWorkingDirectory => TypeRef::string(),
            BuiltinFunction::EnvHomeDirectory => TypeRef::string(),
            BuiltinFunction::EnvPlatform => TypeRef::int(),
            BuiltinFunction::EnvSetWorkingDirectory => TypeRef::nil(),
            BuiltinFunction::EnvTempDirectory => TypeRef::string(),
            BuiltinFunction::EnvVariables => TypeRef::Any,
            BuiltinFunction::Exit => TypeRef::Never,
            BuiltinFunction::FileCopy => TypeRef::int(),
            BuiltinFunction::FileDrop => TypeRef::nil(),
            BuiltinFunction::FileFlush => TypeRef::nil(),
            BuiltinFunction::FileOpen => TypeRef::Any,
            BuiltinFunction::FileRead => TypeRef::int(),
            BuiltinFunction::FileRemove => TypeRef::nil(),
            BuiltinFunction::FileSeek => TypeRef::int(),
            BuiltinFunction::FileSize => TypeRef::int(),
            BuiltinFunction::FileWriteBytes => TypeRef::int(),
            BuiltinFunction::FileWriteString => TypeRef::int(),
            BuiltinFunction::FloatAdd => TypeRef::float(),
            BuiltinFunction::FloatCeil => TypeRef::float(),
            BuiltinFunction::FloatDiv => TypeRef::float(),
            BuiltinFunction::FloatEq => TypeRef::boolean(),
            BuiltinFunction::FloatFloor => TypeRef::float(),
            BuiltinFunction::FloatFromBits => TypeRef::float(),
            BuiltinFunction::FloatGe => TypeRef::boolean(),
            BuiltinFunction::FloatGt => TypeRef::boolean(),
            BuiltinFunction::FloatIsInf => TypeRef::boolean(),
            BuiltinFunction::FloatIsNan => TypeRef::boolean(),
            BuiltinFunction::FloatLe => TypeRef::boolean(),
            BuiltinFunction::FloatLt => TypeRef::boolean(),
            BuiltinFunction::FloatMod => TypeRef::float(),
            BuiltinFunction::FloatMul => TypeRef::float(),
            BuiltinFunction::FloatRound => TypeRef::float(),
            BuiltinFunction::FloatSub => TypeRef::float(),
            BuiltinFunction::FloatToBits => TypeRef::int(),
            BuiltinFunction::FloatToInt => TypeRef::int(),
            BuiltinFunction::FloatToString => TypeRef::string(),
            BuiltinFunction::ChannelReceive => TypeRef::Any,
            BuiltinFunction::ChannelReceiveUntil => TypeRef::Any,
            BuiltinFunction::ChannelDrop => TypeRef::nil(),
            BuiltinFunction::ChannelWait => TypeRef::nil(),
            BuiltinFunction::ChannelNew => TypeRef::Any,
            BuiltinFunction::ChannelSend => TypeRef::nil(),
            BuiltinFunction::ChannelTryReceive => TypeRef::Any,
            BuiltinFunction::GetNil => TypeRef::nil(),
            BuiltinFunction::HasherDrop => TypeRef::nil(),
            BuiltinFunction::HasherNew => TypeRef::Any,
            BuiltinFunction::HasherToHash => TypeRef::int(),
            BuiltinFunction::HasherWriteInt => TypeRef::nil(),
            BuiltinFunction::IntAdd => TypeRef::int(),
            BuiltinFunction::IntBitAnd => TypeRef::int(),
            BuiltinFunction::IntBitNot => TypeRef::int(),
            BuiltinFunction::IntBitOr => TypeRef::int(),
            BuiltinFunction::IntBitXor => TypeRef::int(),
            BuiltinFunction::IntDiv => TypeRef::int(),
            BuiltinFunction::IntEq => TypeRef::boolean(),
            BuiltinFunction::IntGe => TypeRef::boolean(),
            BuiltinFunction::IntGt => TypeRef::boolean(),
            BuiltinFunction::IntLe => TypeRef::boolean(),
            BuiltinFunction::IntLt => TypeRef::boolean(),
            BuiltinFunction::IntRem => TypeRef::int(),
            BuiltinFunction::IntMul => TypeRef::int(),
            BuiltinFunction::IntPow => TypeRef::int(),
            BuiltinFunction::IntRotateLeft => TypeRef::int(),
            BuiltinFunction::IntRotateRight => TypeRef::int(),
            BuiltinFunction::IntShl => TypeRef::int(),
            BuiltinFunction::IntShr => TypeRef::int(),
            BuiltinFunction::IntSub => TypeRef::int(),
            BuiltinFunction::IntToFloat => TypeRef::float(),
            BuiltinFunction::IntToString => TypeRef::string(),
            BuiltinFunction::IntUnsignedShr => TypeRef::int(),
            BuiltinFunction::IntWrappingAdd => TypeRef::int(),
            BuiltinFunction::IntWrappingMul => TypeRef::int(),
            BuiltinFunction::IntWrappingSub => TypeRef::int(),
            BuiltinFunction::IsNull => TypeRef::boolean(),
            BuiltinFunction::Moved => TypeRef::nil(),
            BuiltinFunction::ObjectEq => TypeRef::boolean(),
            BuiltinFunction::Panic => TypeRef::Never,
            BuiltinFunction::PathAccessedAt => TypeRef::float(),
            BuiltinFunction::PathCreatedAt => TypeRef::float(),
            BuiltinFunction::PathExists => TypeRef::boolean(),
            BuiltinFunction::PathIsDirectory => TypeRef::boolean(),
            BuiltinFunction::PathIsFile => TypeRef::boolean(),
            BuiltinFunction::PathModifiedAt => TypeRef::float(),
            BuiltinFunction::ProcessStackFrameLine => TypeRef::int(),
            BuiltinFunction::ProcessStackFrameName => TypeRef::string(),
            BuiltinFunction::ProcessStackFramePath => TypeRef::string(),
            BuiltinFunction::ProcessStacktrace => TypeRef::Any,
            BuiltinFunction::ProcessStacktraceDrop => TypeRef::nil(),
            BuiltinFunction::ProcessStacktraceLength => TypeRef::int(),
            BuiltinFunction::ProcessSuspend => TypeRef::nil(),
            BuiltinFunction::RandomBytes => TypeRef::byte_array(),
            BuiltinFunction::RandomDrop => TypeRef::nil(),
            BuiltinFunction::RandomFloat => TypeRef::float(),
            BuiltinFunction::RandomFloatRange => TypeRef::float(),
            BuiltinFunction::RandomFromInt => TypeRef::Any,
            BuiltinFunction::RandomInt => TypeRef::int(),
            BuiltinFunction::RandomIntRange => TypeRef::int(),
            BuiltinFunction::RandomNew => TypeRef::Any,
            BuiltinFunction::SocketAccept => TypeRef::Any,
            BuiltinFunction::SocketAddressPairAddress => TypeRef::string(),
            BuiltinFunction::SocketAddressPairDrop => TypeRef::nil(),
            BuiltinFunction::SocketAddressPairPort => TypeRef::int(),
            BuiltinFunction::SocketBind => TypeRef::nil(),
            BuiltinFunction::SocketConnect => TypeRef::nil(),
            BuiltinFunction::SocketDrop => TypeRef::nil(),
            BuiltinFunction::SocketListen => TypeRef::nil(),
            BuiltinFunction::SocketLocalAddress => TypeRef::Any,
            BuiltinFunction::SocketNew => TypeRef::Any,
            BuiltinFunction::SocketPeerAddress => TypeRef::Any,
            BuiltinFunction::SocketRead => TypeRef::int(),
            BuiltinFunction::SocketReceiveFrom => TypeRef::Any,
            BuiltinFunction::SocketSendBytesTo => TypeRef::int(),
            BuiltinFunction::SocketSendStringTo => TypeRef::int(),
            BuiltinFunction::SocketSetBroadcast => TypeRef::nil(),
            BuiltinFunction::SocketSetKeepalive => TypeRef::nil(),
            BuiltinFunction::SocketSetLinger => TypeRef::nil(),
            BuiltinFunction::SocketSetNodelay => TypeRef::nil(),
            BuiltinFunction::SocketSetOnlyV6 => TypeRef::nil(),
            BuiltinFunction::SocketSetRecvSize => TypeRef::nil(),
            BuiltinFunction::SocketSetReuseAddress => TypeRef::nil(),
            BuiltinFunction::SocketSetReusePort => TypeRef::nil(),
            BuiltinFunction::SocketSetSendSize => TypeRef::nil(),
            BuiltinFunction::SocketSetTtl => TypeRef::nil(),
            BuiltinFunction::SocketShutdownRead => TypeRef::nil(),
            BuiltinFunction::SocketShutdownReadWrite => TypeRef::nil(),
            BuiltinFunction::SocketShutdownWrite => TypeRef::nil(),
            BuiltinFunction::SocketTryClone => TypeRef::Any,
            BuiltinFunction::SocketWriteBytes => TypeRef::int(),
            BuiltinFunction::SocketWriteString => TypeRef::int(),
            BuiltinFunction::StderrFlush => TypeRef::nil(),
            BuiltinFunction::StderrWriteBytes => TypeRef::int(),
            BuiltinFunction::StderrWriteString => TypeRef::int(),
            BuiltinFunction::StdinRead => TypeRef::int(),
            BuiltinFunction::StdoutFlush => TypeRef::nil(),
            BuiltinFunction::StdoutWriteBytes => TypeRef::int(),
            BuiltinFunction::StdoutWriteString => TypeRef::int(),
            BuiltinFunction::StringByte => TypeRef::int(),
            BuiltinFunction::StringCharacters => TypeRef::Any,
            BuiltinFunction::StringCharactersDrop => TypeRef::nil(),
            BuiltinFunction::StringCharactersNext => TypeRef::Any,
            BuiltinFunction::StringConcat => TypeRef::string(),
            BuiltinFunction::StringConcatArray => TypeRef::string(),
            BuiltinFunction::StringDrop => TypeRef::nil(),
            BuiltinFunction::StringEq => TypeRef::boolean(),
            BuiltinFunction::StringSize => TypeRef::int(),
            BuiltinFunction::StringSliceBytes => TypeRef::string(),
            BuiltinFunction::StringToByteArray => TypeRef::byte_array(),
            BuiltinFunction::StringToFloat => TypeRef::float(),
            BuiltinFunction::StringToInt => TypeRef::int(),
            BuiltinFunction::StringToLower => TypeRef::string(),
            BuiltinFunction::StringToUpper => TypeRef::string(),
            BuiltinFunction::TimeMonotonic => TypeRef::int(),
            BuiltinFunction::TimeSystem => TypeRef::float(),
            BuiltinFunction::TimeSystemOffset => TypeRef::int(),
            BuiltinFunction::PanicThrown => TypeRef::Never,
        }
    }

    pub fn throw_type(self) -> TypeRef {
        match self {
            BuiltinFunction::ArrayCapacity => TypeRef::Never,
            BuiltinFunction::ArrayClear => TypeRef::Never,
            BuiltinFunction::ArrayDrop => TypeRef::Never,
            BuiltinFunction::ArrayGet => TypeRef::Never,
            BuiltinFunction::ArrayLength => TypeRef::Never,
            BuiltinFunction::ArrayPop => TypeRef::Never,
            BuiltinFunction::ArrayPush => TypeRef::Never,
            BuiltinFunction::ArrayRemove => TypeRef::Never,
            BuiltinFunction::ArrayReserve => TypeRef::Never,
            BuiltinFunction::ArraySet => TypeRef::Never,
            BuiltinFunction::ByteArrayAppend => TypeRef::Never,
            BuiltinFunction::ByteArrayClear => TypeRef::Never,
            BuiltinFunction::ByteArrayClone => TypeRef::Never,
            BuiltinFunction::ByteArrayCopyFrom => TypeRef::Never,
            BuiltinFunction::ByteArrayDrainToString => TypeRef::Never,
            BuiltinFunction::ByteArrayDrop => TypeRef::Never,
            BuiltinFunction::ByteArrayEq => TypeRef::Never,
            BuiltinFunction::ByteArrayGet => TypeRef::Never,
            BuiltinFunction::ByteArrayLength => TypeRef::Never,
            BuiltinFunction::ByteArrayNew => TypeRef::Never,
            BuiltinFunction::ByteArrayPop => TypeRef::Never,
            BuiltinFunction::ByteArrayPush => TypeRef::Never,
            BuiltinFunction::ByteArrayRemove => TypeRef::Never,
            BuiltinFunction::ByteArrayResize => TypeRef::Never,
            BuiltinFunction::ByteArraySet => TypeRef::Never,
            BuiltinFunction::ByteArraySlice => TypeRef::Never,
            BuiltinFunction::ByteArrayToString => TypeRef::Never,
            BuiltinFunction::ChannelDrop => TypeRef::Never,
            BuiltinFunction::ChannelNew => TypeRef::Never,
            BuiltinFunction::ChannelReceive => TypeRef::Never,
            BuiltinFunction::ChannelReceiveUntil => TypeRef::Any,
            BuiltinFunction::ChannelSend => TypeRef::Never,
            BuiltinFunction::ChannelTryReceive => TypeRef::Any,
            BuiltinFunction::ChannelWait => TypeRef::Never,
            BuiltinFunction::ChildProcessDrop => TypeRef::Never,
            BuiltinFunction::ChildProcessSpawn => TypeRef::int(),
            BuiltinFunction::ChildProcessStderrClose => TypeRef::Never,
            BuiltinFunction::ChildProcessStderrRead => TypeRef::int(),
            BuiltinFunction::ChildProcessStdinClose => TypeRef::Never,
            BuiltinFunction::ChildProcessStdinFlush => TypeRef::int(),
            BuiltinFunction::ChildProcessStdinWriteBytes => TypeRef::int(),
            BuiltinFunction::ChildProcessStdinWriteString => TypeRef::int(),
            BuiltinFunction::ChildProcessStdoutClose => TypeRef::Never,
            BuiltinFunction::ChildProcessStdoutRead => TypeRef::int(),
            BuiltinFunction::ChildProcessTryWait => TypeRef::int(),
            BuiltinFunction::ChildProcessWait => TypeRef::int(),
            BuiltinFunction::CpuCores => TypeRef::Never,
            BuiltinFunction::DirectoryCreate => TypeRef::int(),
            BuiltinFunction::DirectoryCreateRecursive => TypeRef::int(),
            BuiltinFunction::DirectoryList => TypeRef::int(),
            BuiltinFunction::DirectoryRemove => TypeRef::int(),
            BuiltinFunction::DirectoryRemoveRecursive => TypeRef::int(),
            BuiltinFunction::EnvArguments => TypeRef::Never,
            BuiltinFunction::EnvExecutable => TypeRef::int(),
            BuiltinFunction::EnvGet => TypeRef::Never,
            BuiltinFunction::EnvGetWorkingDirectory => TypeRef::int(),
            BuiltinFunction::EnvHomeDirectory => TypeRef::Never,
            BuiltinFunction::EnvPlatform => TypeRef::Never,
            BuiltinFunction::EnvSetWorkingDirectory => TypeRef::int(),
            BuiltinFunction::EnvTempDirectory => TypeRef::Never,
            BuiltinFunction::EnvVariables => TypeRef::Never,
            BuiltinFunction::Exit => TypeRef::Never,
            BuiltinFunction::FileCopy => TypeRef::int(),
            BuiltinFunction::FileDrop => TypeRef::Never,
            BuiltinFunction::FileFlush => TypeRef::int(),
            BuiltinFunction::FileOpen => TypeRef::int(),
            BuiltinFunction::FileRead => TypeRef::int(),
            BuiltinFunction::FileRemove => TypeRef::int(),
            BuiltinFunction::FileSeek => TypeRef::int(),
            BuiltinFunction::FileSize => TypeRef::int(),
            BuiltinFunction::FileWriteBytes => TypeRef::int(),
            BuiltinFunction::FileWriteString => TypeRef::int(),
            BuiltinFunction::FloatAdd => TypeRef::Never,
            BuiltinFunction::FloatCeil => TypeRef::Never,
            BuiltinFunction::FloatDiv => TypeRef::Never,
            BuiltinFunction::FloatEq => TypeRef::Never,
            BuiltinFunction::FloatFloor => TypeRef::Never,
            BuiltinFunction::FloatFromBits => TypeRef::Never,
            BuiltinFunction::FloatGe => TypeRef::Never,
            BuiltinFunction::FloatGt => TypeRef::Never,
            BuiltinFunction::FloatIsInf => TypeRef::Never,
            BuiltinFunction::FloatIsNan => TypeRef::Never,
            BuiltinFunction::FloatLe => TypeRef::Never,
            BuiltinFunction::FloatLt => TypeRef::Never,
            BuiltinFunction::FloatMod => TypeRef::Never,
            BuiltinFunction::FloatMul => TypeRef::Never,
            BuiltinFunction::FloatRound => TypeRef::Never,
            BuiltinFunction::FloatSub => TypeRef::Never,
            BuiltinFunction::FloatToBits => TypeRef::Never,
            BuiltinFunction::FloatToInt => TypeRef::Never,
            BuiltinFunction::FloatToString => TypeRef::Never,
            BuiltinFunction::GetNil => TypeRef::Never,
            BuiltinFunction::HasherDrop => TypeRef::Never,
            BuiltinFunction::HasherNew => TypeRef::Never,
            BuiltinFunction::HasherToHash => TypeRef::Never,
            BuiltinFunction::HasherWriteInt => TypeRef::Never,
            BuiltinFunction::IntAdd => TypeRef::Never,
            BuiltinFunction::IntBitAnd => TypeRef::Never,
            BuiltinFunction::IntBitNot => TypeRef::Never,
            BuiltinFunction::IntBitOr => TypeRef::Never,
            BuiltinFunction::IntBitXor => TypeRef::Never,
            BuiltinFunction::IntDiv => TypeRef::Never,
            BuiltinFunction::IntEq => TypeRef::Never,
            BuiltinFunction::IntGe => TypeRef::Never,
            BuiltinFunction::IntGt => TypeRef::Never,
            BuiltinFunction::IntLe => TypeRef::Never,
            BuiltinFunction::IntLt => TypeRef::Never,
            BuiltinFunction::IntMul => TypeRef::Never,
            BuiltinFunction::IntPow => TypeRef::Never,
            BuiltinFunction::IntRem => TypeRef::Never,
            BuiltinFunction::IntRotateLeft => TypeRef::Never,
            BuiltinFunction::IntRotateRight => TypeRef::Never,
            BuiltinFunction::IntShl => TypeRef::Never,
            BuiltinFunction::IntShr => TypeRef::Never,
            BuiltinFunction::IntSub => TypeRef::Never,
            BuiltinFunction::IntToFloat => TypeRef::Never,
            BuiltinFunction::IntToString => TypeRef::Never,
            BuiltinFunction::IntUnsignedShr => TypeRef::Never,
            BuiltinFunction::IntWrappingAdd => TypeRef::Never,
            BuiltinFunction::IntWrappingMul => TypeRef::Never,
            BuiltinFunction::IntWrappingSub => TypeRef::Never,
            BuiltinFunction::IsNull => TypeRef::Never,
            BuiltinFunction::Moved => TypeRef::Never,
            BuiltinFunction::ObjectEq => TypeRef::Never,
            BuiltinFunction::Panic => TypeRef::Never,
            BuiltinFunction::PanicThrown => TypeRef::Never,
            BuiltinFunction::PathAccessedAt => TypeRef::int(),
            BuiltinFunction::PathCreatedAt => TypeRef::int(),
            BuiltinFunction::PathExists => TypeRef::Never,
            BuiltinFunction::PathIsDirectory => TypeRef::Never,
            BuiltinFunction::PathIsFile => TypeRef::Never,
            BuiltinFunction::PathModifiedAt => TypeRef::int(),
            BuiltinFunction::ProcessStackFrameLine => TypeRef::Never,
            BuiltinFunction::ProcessStackFrameName => TypeRef::Never,
            BuiltinFunction::ProcessStackFramePath => TypeRef::Never,
            BuiltinFunction::ProcessStacktrace => TypeRef::Never,
            BuiltinFunction::ProcessStacktraceDrop => TypeRef::Never,
            BuiltinFunction::ProcessStacktraceLength => TypeRef::Never,
            BuiltinFunction::ProcessSuspend => TypeRef::Never,
            BuiltinFunction::RandomBytes => TypeRef::Never,
            BuiltinFunction::RandomDrop => TypeRef::Never,
            BuiltinFunction::RandomFloat => TypeRef::Never,
            BuiltinFunction::RandomFloatRange => TypeRef::Never,
            BuiltinFunction::RandomFromInt => TypeRef::Never,
            BuiltinFunction::RandomInt => TypeRef::Never,
            BuiltinFunction::RandomIntRange => TypeRef::Never,
            BuiltinFunction::RandomNew => TypeRef::Never,
            BuiltinFunction::SocketAccept => TypeRef::int(),
            BuiltinFunction::SocketAddressPairAddress => TypeRef::Never,
            BuiltinFunction::SocketAddressPairDrop => TypeRef::Never,
            BuiltinFunction::SocketAddressPairPort => TypeRef::Never,
            BuiltinFunction::SocketBind => TypeRef::int(),
            BuiltinFunction::SocketConnect => TypeRef::int(),
            BuiltinFunction::SocketDrop => TypeRef::Never,
            BuiltinFunction::SocketListen => TypeRef::int(),
            BuiltinFunction::SocketLocalAddress => TypeRef::int(),
            BuiltinFunction::SocketNew => TypeRef::int(),
            BuiltinFunction::SocketPeerAddress => TypeRef::int(),
            BuiltinFunction::SocketRead => TypeRef::int(),
            BuiltinFunction::SocketReceiveFrom => TypeRef::int(),
            BuiltinFunction::SocketSendBytesTo => TypeRef::int(),
            BuiltinFunction::SocketSendStringTo => TypeRef::int(),
            BuiltinFunction::SocketSetBroadcast => TypeRef::int(),
            BuiltinFunction::SocketSetKeepalive => TypeRef::int(),
            BuiltinFunction::SocketSetLinger => TypeRef::int(),
            BuiltinFunction::SocketSetNodelay => TypeRef::int(),
            BuiltinFunction::SocketSetOnlyV6 => TypeRef::int(),
            BuiltinFunction::SocketSetRecvSize => TypeRef::int(),
            BuiltinFunction::SocketSetReuseAddress => TypeRef::int(),
            BuiltinFunction::SocketSetReusePort => TypeRef::int(),
            BuiltinFunction::SocketSetSendSize => TypeRef::int(),
            BuiltinFunction::SocketSetTtl => TypeRef::int(),
            BuiltinFunction::SocketShutdownRead => TypeRef::int(),
            BuiltinFunction::SocketShutdownReadWrite => TypeRef::int(),
            BuiltinFunction::SocketShutdownWrite => TypeRef::int(),
            BuiltinFunction::SocketTryClone => TypeRef::int(),
            BuiltinFunction::SocketWriteBytes => TypeRef::int(),
            BuiltinFunction::SocketWriteString => TypeRef::int(),
            BuiltinFunction::StderrFlush => TypeRef::int(),
            BuiltinFunction::StderrWriteBytes => TypeRef::int(),
            BuiltinFunction::StderrWriteString => TypeRef::int(),
            BuiltinFunction::StdinRead => TypeRef::int(),
            BuiltinFunction::StdoutFlush => TypeRef::int(),
            BuiltinFunction::StdoutWriteBytes => TypeRef::int(),
            BuiltinFunction::StdoutWriteString => TypeRef::int(),
            BuiltinFunction::StringByte => TypeRef::Never,
            BuiltinFunction::StringCharacters => TypeRef::Never,
            BuiltinFunction::StringCharactersDrop => TypeRef::Never,
            BuiltinFunction::StringCharactersNext => TypeRef::Never,
            BuiltinFunction::StringConcat => TypeRef::Never,
            BuiltinFunction::StringConcatArray => TypeRef::Never,
            BuiltinFunction::StringDrop => TypeRef::Never,
            BuiltinFunction::StringEq => TypeRef::Never,
            BuiltinFunction::StringSize => TypeRef::Never,
            BuiltinFunction::StringSliceBytes => TypeRef::Never,
            BuiltinFunction::StringToByteArray => TypeRef::Never,
            BuiltinFunction::StringToFloat => TypeRef::Never,
            BuiltinFunction::StringToInt => TypeRef::Never,
            BuiltinFunction::StringToLower => TypeRef::Never,
            BuiltinFunction::StringToUpper => TypeRef::Never,
            BuiltinFunction::TimeMonotonic => TypeRef::Never,
            BuiltinFunction::TimeSystem => TypeRef::Never,
            BuiltinFunction::TimeSystemOffset => TypeRef::Never,
        }
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

    /// The method is defined using a trait implementation.
    Implementation(TraitInstance, MethodId),
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
    bounds: TypeBounds,
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
            bounds: TypeBounds::new(),
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
pub struct MethodId(pub usize);

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

        let rules = Rules::none();

        ours.type_parameters
            .values()
            .clone()
            .into_iter()
            .zip(theirs.type_parameters.values().clone().into_iter())
            .all(|(ours, theirs)| {
                ours.type_check_with_type_parameter(db, theirs, context, rules)
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

        ours.type_check(db, theirs, context, Rules::new())
    }

    fn type_check_throw_type(
        self,
        db: &mut Database,
        with: MethodId,
        context: &mut TypeContext,
    ) -> bool {
        let ours = self.get(db).throw_type;
        let theirs = with.get(db).throw_type;

        ours.type_check(db, theirs, context, Rules::new())
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

    pub fn is_instance_method(self, db: &Database) -> bool {
        self.kind(db) != MethodKind::Static
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

    pub fn add_argument(self, db: &mut Database, argument: Argument) {
        self.get_mut(db).arguments.new_argument(
            argument.name.clone(),
            argument.value_type,
            argument.variable,
        );
    }

    pub fn set_main(self, db: &mut Database) {
        self.get_mut(db).main = true;
    }

    pub fn is_main(self, db: &Database) -> bool {
        self.get(db).main
    }

    pub fn bounds(self, db: &Database) -> &TypeBounds {
        &self.get(db).bounds
    }

    pub fn set_bounds(self, db: &mut Database, bounds: TypeBounds) {
        self.get_mut(db).bounds = bounds;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Receiver {
    /// The receiver is explicit (e.g. `foo.bar()`)
    Explicit,

    /// The receiver is implicitly `self` (e.g. `bar()` and there's an instance
    /// method with that name).
    Implicit,

    /// The receiver is a class to call a static method on.
    ///
    /// This is separate from an explicit receiver as we don't need to process
    /// the receiver expression in this case.
    Class(ClassId),
}

impl Receiver {
    pub fn class_or_implicit(db: &Database, method: MethodId) -> Receiver {
        method
            .receiver(db)
            .as_class(db)
            .map(Receiver::Class)
            .unwrap_or(Receiver::Implicit)
    }

    pub fn class_or_explicit(db: &Database, receiver: TypeRef) -> Receiver {
        receiver.as_class(db).map(Receiver::Class).unwrap_or(Receiver::Explicit)
    }

    pub fn is_explicit(&self) -> bool {
        matches!(self, Receiver::Explicit)
    }
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
    pub id: BuiltinFunction,
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
    CallClosure(ClosureCallInfo),
    GetField(FieldInfo),
    SetField(FieldInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentifierKind {
    Unknown,
    Variable(VariableId),
    Method(CallInfo),
    Field(FieldInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstantKind {
    Unknown,
    Constant(ConstantId),
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

    pub(crate) fn add(db: &mut Database, closure: Closure) -> ClosureId {
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
        rules: Rules,
    ) -> bool {
        match with {
            TypeId::Closure(with) => {
                self.type_check_arguments(db, with, context)
                    && self.type_check_throw_type(db, with, context, rules)
                    && self.type_check_return_type(db, with, context, rules)
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
        rules: Rules,
    ) -> bool {
        let ours = self.get(db).return_type;
        let theirs = with.get(db).return_type;

        ours.type_check(db, theirs, context, rules)
    }

    fn type_check_throw_type(
        self,
        db: &mut Database,
        with: ClosureId,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        let ours = self.get(db).throw_type;
        let theirs = with.get(db).throw_type;

        ours.type_check(db, theirs, context, rules)
    }

    pub(crate) fn get(self, db: &Database) -> &Closure {
        &db.closures[self.0]
    }

    fn get_mut(self, db: &mut Database) -> &mut Closure {
        &mut db.closures[self.0]
    }

    fn as_rigid_type(self, db: &mut Database, bounds: &TypeBounds) -> Self {
        let mut new_func = self.get(db).clone();

        for arg in new_func.arguments.mapping.values_mut() {
            arg.value_type = arg.value_type.as_rigid_type(db, bounds);
        }

        new_func.throw_type = new_func.throw_type.as_rigid_type(db, bounds);
        new_func.return_type = new_func.return_type.as_rigid_type(db, bounds);

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

/// Rules to apply/enforce when performing type checking.
#[derive(Clone, Copy)]
pub struct Rules {
    /// When set to `true`, subtyping of types through traits is allowed.
    subtyping: bool,

    /// When set to `true`, owned types can be type checked against reference
    /// types.
    relaxed_ownership: bool,
}

impl Rules {
    pub fn new() -> Rules {
        Rules { subtyping: true, relaxed_ownership: false }
    }

    pub fn none() -> Rules {
        Rules { subtyping: false, relaxed_ownership: false }
    }

    pub fn relaxed() -> Rules {
        Rules { subtyping: true, relaxed_ownership: true }
    }

    fn without_subtyping(self) -> Rules {
        Rules { subtyping: false, relaxed_ownership: self.relaxed_ownership }
    }

    fn with_relaxed_ownership(self) -> Rules {
        Rules { subtyping: self.subtyping, relaxed_ownership: true }
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

    pub fn placeholder(
        db: &mut Database,
        required: Option<TypeParameterId>,
    ) -> TypeRef {
        TypeRef::Placeholder(TypePlaceholder::alloc(db, required))
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

    pub fn is_generic(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(TypeId::TraitInstance(ins))
            | TypeRef::Uni(TypeId::TraitInstance(ins))
            | TypeRef::Ref(TypeId::TraitInstance(ins))
            | TypeRef::Mut(TypeId::TraitInstance(ins))
            | TypeRef::RefUni(TypeId::TraitInstance(ins))
            | TypeRef::MutUni(TypeId::TraitInstance(ins)) => {
                ins.instance_of.is_generic(db)
            }
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::RefUni(TypeId::ClassInstance(ins))
            | TypeRef::MutUni(TypeId::ClassInstance(ins)) => {
                ins.instance_of.is_generic(db)
            }
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.is_generic(db))
            }
            _ => false,
        }
    }

    pub fn type_arguments(self, db: &Database) -> TypeArguments {
        match self {
            TypeRef::Owned(TypeId::TraitInstance(ins))
            | TypeRef::Uni(TypeId::TraitInstance(ins))
            | TypeRef::Ref(TypeId::TraitInstance(ins))
            | TypeRef::Mut(TypeId::TraitInstance(ins))
            | TypeRef::RefUni(TypeId::TraitInstance(ins))
            | TypeRef::MutUni(TypeId::TraitInstance(ins)) => {
                ins.type_arguments(db).clone()
            }
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::RefUni(TypeId::ClassInstance(ins))
            | TypeRef::MutUni(TypeId::ClassInstance(ins)) => {
                ins.type_arguments(db).clone()
            }
            TypeRef::Placeholder(id) => id
                .value(db)
                .map(|v| v.type_arguments(db))
                .unwrap_or_else(TypeArguments::new),
            _ => TypeArguments::new(),
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

    pub fn is_mutable(self, db: &Database) -> bool {
        match self {
            TypeRef::Owned(_)
            | TypeRef::Uni(_)
            | TypeRef::Mut(_)
            | TypeRef::Infer(_)
            | TypeRef::Error
            | TypeRef::Unknown => true,
            TypeRef::Placeholder(id) => id.resolve(db).is_mutable(db),
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
        self.class_id_with_self_type(db, self_type)
            .map_or(false, |id| id.is_atomic(db))
    }

    pub fn is_bool(self, db: &Database, self_type: TypeId) -> bool {
        self.is_instance_of(db, ClassId::boolean(), self_type)
    }

    pub fn is_int(self, db: &Database, self_type: TypeId) -> bool {
        self.is_instance_of(db, ClassId::int(), self_type)
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

    pub fn cast_according_to(self, other: Self, db: &Database) -> Self {
        if other.is_uni(db) && self.is_value_type(db) {
            self.as_uni(db)
        } else if other.is_ref(db) {
            self.as_ref(db)
        } else if other.is_mut(db) {
            self.as_mut(db)
        } else if self.is_value_type(db)
            && !self.is_owned_or_uni(db)
            && other.is_owned_or_uni(db)
        {
            self.as_owned(db)
        } else {
            self
        }
    }

    pub fn value_type_as_owned(self, db: &Database) -> Self {
        if self.is_value_type(db) {
            self.as_owned(db)
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

    pub fn allow_as_ref(self, db: &Database) -> bool {
        match self {
            TypeRef::Any => true,
            TypeRef::Owned(_) | TypeRef::Mut(_) | TypeRef::Ref(_) => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.allow_as_ref(db))
            }
            _ => false,
        }
    }

    pub fn allow_as_mut(self, db: &Database) -> bool {
        match self {
            TypeRef::Any => true,
            TypeRef::Owned(TypeId::RigidTypeParameter(id)) => id.is_mutable(db),
            TypeRef::Owned(_) | TypeRef::Mut(_) => true,
            TypeRef::Placeholder(id) => {
                id.value(db).map_or(false, |v| v.allow_as_mut(db))
            }
            _ => false,
        }
    }

    pub fn as_mut(self, db: &Database) -> Self {
        match self {
            TypeRef::Owned(TypeId::RigidTypeParameter(id)) => {
                if id.is_mutable(db) {
                    TypeRef::Mut(TypeId::RigidTypeParameter(id))
                } else {
                    self
                }
            }
            TypeRef::Owned(id) => TypeRef::Mut(id),
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
            TypeRef::Owned(id)
            | TypeRef::Infer(id)
            | TypeRef::Uni(id)
            | TypeRef::Mut(id)
            | TypeRef::Ref(id) => TypeRef::Uni(id),
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

    pub fn as_class(self, db: &Database) -> Option<ClassId> {
        match self {
            TypeRef::Owned(TypeId::Class(id)) => Some(id),
            TypeRef::Owned(TypeId::Module(id)) => Some(id.class(db)),
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
        rules: Rules,
    ) -> bool {
        let key = (self, TypeRef::Owned(TypeId::TypeParameter(parameter)));

        if context.checked.contains(&key) {
            return true;
        }

        context.checked.insert(key);

        let rules = rules.with_relaxed_ownership();

        parameter
            .requirements(db)
            .into_iter()
            .all(|r| self.implements_trait_instance(db, r, context, rules))
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

        self.type_check_directly(db, with, context, Rules::new())
    }

    // TODO: can we get rid of this and use the new resolver?
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
        rules: Rules,
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
                .implements_trait_instance(db, trait_type, context, rules),
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::Infer(id) => {
                id.implements_trait_instance(db, trait_type, context, rules)
            }
            TypeRef::Placeholder(id) => id.value(db).map_or(true, |v| {
                v.implements_trait_instance(db, trait_type, context, rules)
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
                    INT_ID
                        | FLOAT_ID
                        | STRING_ID
                        | BOOLEAN_ID
                        | NIL_ID
                        | CHANNEL_ID
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

    // TODO: remove this method?
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

    pub fn class_id_with_self_type(
        self,
        db: &Database,
        self_type: TypeId,
    ) -> Option<ClassId> {
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
            TypeRef::Placeholder(p) => p
                .value(db)
                .and_then(|v| v.class_id_with_self_type(db, self_type)),
            _ => None,
        }
    }

    pub fn class_id(
        self,
        db: &Database,
        self_class: ClassId,
    ) -> Option<ClassId> {
        match self {
            TypeRef::Owned(TypeId::ClassInstance(ins))
            | TypeRef::Uni(TypeId::ClassInstance(ins))
            | TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins)) => Some(ins.instance_of),
            TypeRef::OwnedSelf | TypeRef::RefSelf | TypeRef::MutSelf => {
                Some(self_class)
            }
            TypeRef::Placeholder(p) => {
                p.value(db).and_then(|v| v.class_id(db, self_class))
            }
            _ => None,
        }
    }

    pub fn type_check(
        self,
        db: &mut Database,
        with: TypeRef,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        // This is used to short-circuit recursive type-checks (instead of them
        // blowing up) that involve the same types. For more information see the
        // documentation of the field itself.
        //
        // TODO: this doesn't account for cases where the types are effectively
        // the same, but their type argument structures are different.
        if context.checked.contains(&(self, with)) {
            return true;
        }

        // The comparison must be inserted first, otherwise recursive
        // type-checks would prevent us from ever inserting it in the first
        // place (as we'd hit the depth limit first).
        context.checked.insert((self, with));

        // If a type-check involves too deeply nested types (but not necessarily
        // comparisons with the same types) we just give up. Such types are
        // extremely rare anyway, and the alternative is overflowing the stack.
        //
        // TODO: because of the above check this is probably no longer needed?
        if context.depth == MAX_TYPE_DEPTH {
            return false;
        }

        context.depth += 1;

        // We special-case type parameters on the right-hand side here, that way
        // we don't need to cover this case for all the various TypeRef variants
        // individually.
        let result = match with {
            TypeRef::Owned(TypeId::TypeParameter(pid))
            | TypeRef::Uni(TypeId::TypeParameter(pid))
            | TypeRef::Infer(TypeId::TypeParameter(pid))
            | TypeRef::RefUni(TypeId::TypeParameter(pid))
            | TypeRef::MutUni(TypeId::TypeParameter(pid))
            | TypeRef::Mut(TypeId::TypeParameter(pid))
            | TypeRef::Ref(TypeId::TypeParameter(pid)) => self
                .type_check_with_type_parameter(db, with, pid, context, rules),
            TypeRef::Placeholder(id) => {
                if let Some(assigned) = id.value(db) {
                    self.type_check_directly(db, assigned, context, rules)
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
            _ => self.type_check_directly(db, with, context, rules),
        };

        context.depth -= 1;
        result
    }

    fn type_check_with_type_parameter(
        self,
        db: &mut Database,
        with: TypeRef,
        param: TypeParameterId,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        if let Some(mut assigned) = context.type_arguments.get(param) {
            // This ensures that if we compare a `Foo` with a `uni T`, where `T`
            // is assigned to `Foo`, we _disallow_ this because `Foo` isn't
            // compatible with `uni Foo`.
            if let TypeRef::Owned(_) = assigned {
                match with {
                    TypeRef::Uni(_) => assigned = assigned.as_uni(db),
                    TypeRef::Ref(_) => assigned = assigned.as_ref(db),
                    TypeRef::Mut(_) => assigned = assigned.as_mut(db),
                    _ => {}
                }
            }

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

                let compat = self.type_check_directly(db, rhs, context, rules);

                if compat && update {
                    placeholder.assign(db, self);
                }

                return compat;
            }

            return self.type_check_directly(
                db,
                assigned.cast_according_to(with, db),
                context,
                rules,
            );
        }

        if self.type_check_directly(db, with, context, rules) {
            context.type_arguments.assign(param, self);
            return true;
        }

        false
    }

    fn type_check_with_type_placeholder(
        self,
        db: &mut Database,
        with: TypePlaceholderId,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        if let Some(assigned) = with.value(db) {
            self.type_check(db, assigned, context, rules)
        } else {
            with.assign(db, self);
            true
        }
    }

    fn type_check_directly(
        self,
        db: &mut Database,
        with: TypeRef,
        context: &mut TypeContext,
        rules: Rules,
    ) -> bool {
        // This case is the same for all variants of `self`, so we handle it
        // here once.
        if let TypeRef::Placeholder(id) = with {
            return self
                .type_check_with_type_placeholder(db, id, context, rules);
        }

        match self {
            TypeRef::Owned(our_id) => match with {
                TypeRef::Owned(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Ref(their_id) | TypeRef::Mut(their_id)
                    if self.is_value_type(db) || rules.relaxed_ownership =>
                {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Any | TypeRef::RefAny | TypeRef::Error => true,
                TypeRef::OwnedSelf => {
                    our_id.type_check(db, context.self_type, context, rules)
                }
                _ => false,
            },
            TypeRef::Uni(our_id) => match with {
                TypeRef::Owned(their_id)
                | TypeRef::Infer(their_id)
                | TypeRef::Uni(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Any | TypeRef::RefAny | TypeRef::Error => true,
                TypeRef::UniSelf => {
                    our_id.type_check(db, context.self_type, context, rules)
                }
                _ => false,
            },
            TypeRef::RefUni(our_id) => match with {
                TypeRef::RefUni(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::MutUni(our_id) => match with {
                TypeRef::RefUni(their_id) | TypeRef::MutUni(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Ref(our_id) => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                // Consider this implementation:
                //
                //     impl Equal[ref Thing] for Thing { ... }
                //
                // And the following method:
                //
                //     fn example[T: Equal[T]](a: ref T) { ... }
                //
                // If we pass this a `ref Array[mut Thing]`, we end up comparing
                // the `Equal[ref Thing]` implementation with the expected
                // implementation `Equal[mut Thing]`.
                //
                // Normally this is invalid, but in the above context it's sound
                // as the `example` method is restricted by the requirements as
                // it specifies them (i.e. it doesn't know if `T` is actually
                // owned, a ref, etc). As such we allow this comparison if
                // needed.
                TypeRef::Mut(their_id) if rules.relaxed_ownership => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Owned(their_id) | TypeRef::Uni(their_id)
                    if self.is_value_type(db) =>
                {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Error => true,
                TypeRef::RefSelf => {
                    our_id.type_check(db, context.self_type, context, rules)
                }
                _ => false,
            },
            TypeRef::Mut(our_id) => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Mut(their_id) => our_id.type_check(
                    db,
                    their_id,
                    context,
                    rules.without_subtyping(),
                ),
                TypeRef::Owned(their_id) | TypeRef::Uni(their_id)
                    if self.is_value_type(db) =>
                {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Error => true,
                TypeRef::RefSelf => {
                    our_id.type_check(db, context.self_type, context, rules)
                }
                TypeRef::MutSelf => our_id.type_check(
                    db,
                    context.self_type,
                    context,
                    rules.without_subtyping(),
                ),
                _ => false,
            },
            TypeRef::Infer(TypeId::TypeParameter(our_id)) => match with {
                TypeRef::Infer(their_id) => {
                    our_id.type_check(db, their_id, context, rules)
                }
                TypeRef::Error => true,
                _ => {
                    // If our parameter is bound to an argument, we have to
                    // compare that argument with the value on the right
                    // instead.
                    if let Some(arg) = context
                        .type_arguments
                        .get(our_id)
                        .filter(|&arg| self != arg)
                    {
                        return arg.type_check(db, with, context, rules);
                    }

                    false
                }
            },
            // Since a Never can't actually be passed around, it's compatible
            // with everything else. This allows for code like this:
            //
            //     try foo else panic
            //
            // Where `panic` would return a `Never`.
            TypeRef::Never => true,
            TypeRef::OwnedSelf => match with {
                TypeRef::Owned(their_id) | TypeRef::Infer(their_id) => {
                    context.self_type.type_check(db, their_id, context, rules)
                }
                TypeRef::Any
                | TypeRef::RefAny
                | TypeRef::Error
                | TypeRef::OwnedSelf => true,
                _ => false,
            },
            TypeRef::RefSelf => match with {
                TypeRef::Ref(their_id) | TypeRef::Infer(their_id) => {
                    context.self_type.type_check(db, their_id, context, rules)
                }
                TypeRef::Error | TypeRef::RefSelf => true,
                _ => false,
            },
            TypeRef::MutSelf => match with {
                TypeRef::Mut(their_id) | TypeRef::Infer(their_id) => {
                    context.self_type.type_check(
                        db,
                        their_id,
                        context,
                        rules.without_subtyping(),
                    )
                }
                TypeRef::Error | TypeRef::MutSelf => true,
                _ => false,
            },
            TypeRef::UniSelf => match with {
                TypeRef::Owned(their_id)
                | TypeRef::Uni(their_id)
                | TypeRef::Infer(their_id) => context.self_type.type_check(
                    db,
                    their_id,
                    context,
                    rules.without_subtyping(),
                ),
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
                    return assigned.type_check(db, with, context, rules);
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

    fn is_instance_of(
        self,
        db: &Database,
        id: ClassId,
        self_type: TypeId,
    ) -> bool {
        self.class_id_with_self_type(db, self_type) == Some(id)
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
            let is_ins = !matches!(
                self,
                TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_)
            );

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
        rules: Rules,
    ) -> bool {
        match self {
            TypeId::ClassInstance(id) => id
                .type_check_with_trait_instance(db, trait_type, context, rules),
            TypeId::TraitInstance(id) => {
                id.implements_trait_instance(db, trait_type, context)
            }
            TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id) => id
                .type_check_with_trait_instance(db, trait_type, context, rules),
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
        rules: Rules,
    ) -> bool {
        match self {
            TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_) => {
                self == with
            }
            TypeId::ClassInstance(ins) => {
                ins.type_check(db, with, context, rules)
            }
            TypeId::TraitInstance(ins) => {
                ins.type_check(db, with, context, rules)
            }
            TypeId::TypeParameter(ins) => {
                ins.type_check(db, with, context, rules)
            }
            TypeId::RigidTypeParameter(ours) => match with {
                TypeId::RigidTypeParameter(theirs) => ours == theirs,
                _ => ours.type_check(db, with, context, rules),
            },
            TypeId::Closure(ins) => ins.type_check(db, with, context, rules),
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
    builtin_functions: HashMap<String, BuiltinFunction>,
    type_placeholders: Vec<TypePlaceholder>,
    variants: Vec<Variant>,

    /// The module that acts as the entry point of the program.
    ///
    /// For executables this will be set based on the file that is built/run.
    /// When just type-checking a project, this may be left as a None.
    main_module: Option<ModuleName>,
    main_method: Option<MethodId>,
    main_class: Option<ClassId>,
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
                Class::atomic(STRING_NAME.to_string()),
                Class::regular(ARRAY_NAME.to_string()),
                Class::regular(BOOLEAN_NAME.to_string()),
                Class::regular(NIL_NAME.to_string()),
                Class::regular(BYTE_ARRAY_NAME.to_string()),
                Class::atomic(CHANNEL_NAME.to_string()),
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
            builtin_functions: BuiltinFunction::mapping(),
            type_placeholders: Vec::new(),
            variants: Vec::new(),
            main_module: None,
            main_method: None,
            main_class: None,
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
            CHANNEL_NAME => Some(ClassId(CHANNEL_ID)),
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

    pub fn builtin_function(&self, name: &str) -> Option<BuiltinFunction> {
        self.builtin_functions.get(name).cloned()
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

    pub fn set_main_method(&mut self, id: MethodId) {
        self.main_method = Some(id);
    }

    pub fn main_method(&self) -> Option<MethodId> {
        self.main_method
    }

    pub fn set_main_class(&mut self, id: ClassId) {
        self.main_class = Some(id);
    }

    pub fn main_class(&self) -> Option<ClassId> {
        self.main_class
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{
        immutable, instance, mutable, new_parameter, owned, placeholder, rigid,
        uni,
    };
    use std::mem::size_of;

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<TypeId>(), 16);
        assert_eq!(size_of::<TypeRef>(), 24);
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

        assert_eq!(new_arg.value_type, param.as_owned_rigid(),);
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
        let rules = Rules::new();

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
            &mut ctx,
            rules,
        ));
        assert!(TypeRef::Ref(string_ins).implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx,
            rules,
        ));
        assert!(TypeRef::OwnedSelf.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx,
            rules,
        ));
        assert!(TypeRef::RefSelf.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx,
            rules,
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
        let rules = Rules::new();

        assert!(TypeRef::Owned(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
        assert!(TypeRef::Owned(to_foo_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
        assert!(TypeRef::Ref(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
        assert!(TypeRef::Infer(debug_ins)
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
        assert!(TypeRef::OwnedSelf
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
        assert!(TypeRef::RefSelf
            .implements_trait_instance(&mut db, to_s_ins, &mut ctx, rules));
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
        let rules = Rules::new();

        assert!(param_ins
            .implements_trait_instance(&mut db, debug_ins, &mut ctx, rules));
        assert!(param_ins.implements_trait_instance(
            &mut db,
            to_string_ins,
            &mut ctx,
            rules
        ));
        assert!(!param_ins
            .implements_trait_instance(&mut db, to_foo_ins, &mut ctx, rules));
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
        let rules = Rules::new();

        assert!(!TypeRef::Any
            .implements_trait_instance(&mut db, ins, &mut ctx, rules));
        assert!(!TypeRef::Unknown
            .implements_trait_instance(&mut db, ins, &mut ctx, rules));
        assert!(TypeRef::Error
            .implements_trait_instance(&mut db, ins, &mut ctx, rules));
        assert!(TypeRef::Never
            .implements_trait_instance(&mut db, ins, &mut ctx, rules));
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
        let rules = Rules::new();

        assert!(!closure
            .implements_trait_instance(&mut db, debug_ins, &mut ctx, rules));
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
        assert_eq!(&db.classes[7].name, CHANNEL_NAME);
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
    fn test_class_id_is_builtin() {
        assert!(ClassId::int().is_builtin());
        assert!(!ClassId::tuple8().is_builtin());
        assert!(!ClassId(42).is_builtin());
    }

    #[test]
    fn test_type_placeholder_id_assign() {
        let mut db = Database::new();
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let p1 = TypePlaceholder::alloc(&mut db, Some(param));
        let p2 = TypePlaceholder::alloc(&mut db, Some(param));

        p1.assign(&db, TypeRef::Any);
        p2.assign(&db, TypeRef::Placeholder(p2));

        assert_eq!(p1.value(&db), Some(TypeRef::Any));
        assert!(p2.value(&db).is_none());
    }

    #[test]
    fn test_type_placeholder_id_resolve() {
        let mut db = Database::new();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let var2 = TypePlaceholder::alloc(&mut db, None);
        let var3 = TypePlaceholder::alloc(&mut db, None);

        var1.assign(&db, TypeRef::Any);
        var2.assign(&db, TypeRef::Placeholder(var1));
        var3.assign(&db, TypeRef::Placeholder(var2));

        assert_eq!(var1.resolve(&db), TypeRef::Any);
        assert_eq!(var2.resolve(&db), TypeRef::Any);
        assert_eq!(var3.resolve(&db), TypeRef::Any);
    }

    #[test]
    fn test_type_ref_allow_as_ref() {
        let mut db = Database::new();
        let int = ClassId::int();
        let var = TypePlaceholder::alloc(&mut db, None);
        let param = new_parameter(&mut db, "A");

        var.assign(&db, owned(instance(int)));

        assert!(owned(instance(int)).allow_as_ref(&db));
        assert!(mutable(instance(int)).allow_as_ref(&db));
        assert!(immutable(instance(int)).allow_as_ref(&db));
        assert!(placeholder(var).allow_as_ref(&db));
        assert!(owned(rigid(param)).allow_as_ref(&db));
        assert!(TypeRef::Any.allow_as_ref(&db));
        assert!(!uni(instance(int)).allow_as_ref(&db));
    }

    #[test]
    fn test_type_ref_allow_as_mut() {
        let mut db = Database::new();
        let int = ClassId::int();
        let var = TypePlaceholder::alloc(&mut db, None);
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "A");

        param2.set_mutable(&mut db);
        var.assign(&db, owned(instance(int)));

        assert!(owned(instance(int)).allow_as_mut(&db));
        assert!(mutable(instance(int)).allow_as_mut(&db));
        assert!(placeholder(var).allow_as_mut(&db));
        assert!(TypeRef::Any.allow_as_mut(&db));
        assert!(owned(rigid(param2)).allow_as_mut(&db));
        assert!(!immutable(instance(int)).allow_as_mut(&db));
        assert!(!owned(rigid(param1)).allow_as_mut(&db));
        assert!(!uni(instance(int)).allow_as_mut(&db));
    }

    #[test]
    fn test_type_ref_as_ref() {
        let mut db = Database::new();
        let int = ClassId::int();
        let param = new_parameter(&mut db, "A");

        assert_eq!(owned(instance(int)).as_ref(&db), immutable(instance(int)));
        assert_eq!(
            uni(instance(int)).as_ref(&db),
            TypeRef::RefUni(instance(int))
        );
        assert_eq!(owned(rigid(param)).as_ref(&db), immutable(rigid(param)));
    }

    #[test]
    fn test_type_ref_as_mut() {
        let mut db = Database::new();
        let int = ClassId::int();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "A");

        param2.set_mutable(&mut db);

        assert_eq!(owned(instance(int)).as_mut(&db), mutable(instance(int)));
        assert_eq!(
            uni(instance(int)).as_mut(&db),
            TypeRef::MutUni(instance(int))
        );
        assert_eq!(owned(rigid(param1)).as_mut(&db), owned(rigid(param1)));
        assert_eq!(owned(rigid(param2)).as_mut(&db), mutable(rigid(param2)));
    }
}
