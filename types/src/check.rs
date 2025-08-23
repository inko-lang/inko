use crate::{
    Arguments, Database, ForeignType, MethodId, Ownership, TraitInstance,
    TypeArguments, TypeBounds, TypeEnum, TypeInstance, TypeParameterId,
    TypePlaceholderId, TypeRef, FLOAT_ID, INT_ID,
};
use std::collections::HashSet;

#[derive(Copy, Clone)]
enum Subtyping {
    No,
    Relaxed,
    Strict,
}

#[derive(Copy, Clone)]
enum Kind {
    /// A regular type check.
    Regular,

    /// A type check as part of a type cast.
    Cast,

    /// A type check for a return value.
    Return,
}

impl Kind {
    fn is_return(self) -> bool {
        matches!(self, Kind::Return)
    }

    fn is_cast(self) -> bool {
        matches!(self, Kind::Cast)
    }
}

#[derive(Copy, Clone)]
struct Rules {
    /// The rules to apply when performing sub-typing checks.
    subtyping: Subtyping,

    /// If the root/outer-most type is implicitly compatible with a reference
    /// (i.e. `T -> ref T` is allowed).
    implicit_root_ref: bool,

    /// If a `uni T` is compatible with a `T` value.
    uni_compatible_with_owned: bool,

    /// If type parameters should be turned into rigid parameters in various
    /// contexts (e.g. when comparing trait implementations).
    rigid_parameters: bool,

    /// If the `Never` type is allowed.
    allow_never: bool,

    /// What kind of type check we're performing.
    kind: Kind,
}

impl Rules {
    fn new() -> Rules {
        Rules {
            subtyping: Subtyping::No,
            implicit_root_ref: false,
            uni_compatible_with_owned: true,
            rigid_parameters: false,
            allow_never: true,
            kind: Kind::Regular,
        }
    }

    fn without_never(mut self) -> Rules {
        self.allow_never = false;
        self
    }

    fn without_subtyping(mut self) -> Rules {
        if let Subtyping::Relaxed = self.subtyping {
            self.subtyping = Subtyping::No
        }

        self
    }

    fn infer_as_rigid(mut self) -> Rules {
        self.rigid_parameters = true;
        self
    }

    fn dont_infer_as_rigid(mut self) -> Rules {
        self.rigid_parameters = false;
        self
    }

    fn with_kind(mut self, kind: Kind) -> Rules {
        self.kind = kind;
        self
    }

    fn with_strict_subtyping(mut self) -> Rules {
        self.subtyping = Subtyping::Strict;
        self
    }

    fn with_relaxed_subtyping(mut self) -> Rules {
        self.subtyping = Subtyping::Relaxed;
        self
    }

    fn with_implicit_root_ref(mut self) -> Rules {
        self.implicit_root_ref = true;
        self
    }

    fn without_implicit_root_ref(mut self) -> Rules {
        self.implicit_root_ref = false;
        self
    }
}

/// The type-checking environment.
///
/// This structure contains the type arguments to expose to types that are
/// checked.
#[derive(Clone)]
pub struct Environment {
    /// The type arguments to expose to types on the left-hand side of the
    /// check.
    pub left: TypeArguments,

    /// The type arguments to expose to types on the right-hand side of the
    /// check.
    pub right: TypeArguments,

    /// The type to use for `Self` on the left-hand side.
    pub left_self: Option<TypeEnum>,

    /// The type to use for `Self` on the right-hand side.
    pub right_self: Option<TypeEnum>,

    /// If the type on the left should be treated as a rigid type.
    pub left_rigid: bool,
}

impl Environment {
    pub fn for_types(
        db: &Database,
        left: TypeRef,
        right: TypeRef,
    ) -> Environment {
        Environment::new(left.type_arguments(db), right.type_arguments(db))
    }

    pub fn new(
        left_arguments: TypeArguments,
        right_arguments: TypeArguments,
    ) -> Environment {
        Environment {
            left: left_arguments,
            right: right_arguments,
            left_self: None,
            right_self: None,
            left_rigid: false,
        }
    }

    pub fn with_self_type(
        left_arguments: TypeArguments,
        right_arguments: TypeArguments,
        self_type: TypeEnum,
    ) -> Environment {
        Environment {
            left: left_arguments,
            right: right_arguments,
            left_self: Some(self_type),
            right_self: Some(self_type),
            left_rigid: false,
        }
    }

    pub fn with_right_self_type(
        left_arguments: TypeArguments,
        right_arguments: TypeArguments,
        self_type: TypeEnum,
    ) -> Environment {
        Environment {
            left: left_arguments,
            right: right_arguments,
            left_self: None,
            right_self: Some(self_type),
            left_rigid: false,
        }
    }

    pub fn with_left_as_rigid(mut self) -> Self {
        self.left_rigid = true;
        self
    }

    fn with_left_as_right(&self) -> Environment {
        Environment {
            left: self.left.clone(),
            right: self.left.clone(),
            left_self: None,
            right_self: None,
            left_rigid: false,
        }
    }
}

/// A type for checking if two types are compatible with each other.
pub struct TypeChecker<'a> {
    db: &'a Database,
    checked: HashSet<(TypeRef, TypeRef)>,
}

impl<'a> TypeChecker<'a> {
    pub fn check(db: &'a Database, left: TypeRef, right: TypeRef) -> bool {
        let mut env =
            Environment::new(left.type_arguments(db), right.type_arguments(db));

        TypeChecker::new(db).run(left, right, &mut env)
    }

    pub fn check_cast(db: &'a Database, left: TypeRef, right: TypeRef) -> bool {
        let mut env =
            Environment::new(left.type_arguments(db), right.type_arguments(db));

        let rules = Rules::new().with_kind(Kind::Cast).with_strict_subtyping();

        TypeChecker::new(db).check_type_ref(left, right, &mut env, rules)
    }

    pub fn check_return(
        db: &'a Database,
        left: TypeRef,
        right: TypeRef,
        self_type: TypeEnum,
    ) -> bool {
        let rules = Rules::new().with_kind(Kind::Return);
        let mut env = Environment::with_self_type(
            left.type_arguments(db),
            right.type_arguments(db),
            self_type,
        );

        TypeChecker::new(db).check_type_ref(left, right, &mut env, rules)
    }

    pub fn new(db: &'a Database) -> TypeChecker<'a> {
        TypeChecker { db, checked: HashSet::new() }
    }

    pub fn run(
        mut self,
        left: TypeRef,
        right: TypeRef,
        env: &mut Environment,
    ) -> bool {
        self.check_type_ref(left, right, env, Rules::new())
    }

    pub fn check_type_argument(
        mut self,
        left: TypeRef,
        right: TypeRef,
        env: &mut Environment,
    ) -> bool {
        self.check_type_ref(left, right, env, Rules::new().without_never())
    }

    pub fn check_argument(
        mut self,
        left: TypeRef,
        right: TypeRef,
        env: &mut Environment,
    ) -> bool {
        self.check_type_ref(
            left,
            right,
            env,
            Rules::new().without_subtyping().with_implicit_root_ref(),
        )
    }

    pub fn check_method(
        mut self,
        left: MethodId,
        right: MethodId,
        env: &mut Environment,
    ) -> bool {
        let rules = Rules::new();
        let lhs = left.get(self.db);
        let rhs = right.get(self.db);

        if lhs.kind != rhs.kind {
            return false;
        }

        if lhs.visibility != rhs.visibility {
            return false;
        }

        if lhs.name != rhs.name {
            return false;
        }

        if lhs.type_parameters.len() != rhs.type_parameters.len() {
            return false;
        }

        if !lhs
            .type_parameters
            .values()
            .zip(rhs.type_parameters.values())
            .all(|(&lhs, &rhs)| self.check_parameters(lhs, rhs, env, rules))
        {
            return false;
        }

        if !self.check_arguments(
            &lhs.arguments,
            &rhs.arguments,
            env,
            rules,
            true,
        ) {
            return false;
        }

        self.check_type_ref(
            lhs.return_type,
            rhs.return_type,
            env,
            rules.with_strict_subtyping(),
        )
    }

    pub fn check_bounds(
        &mut self,
        bounds: &TypeBounds,
        env: &mut Environment,
    ) -> bool {
        // When verifying bounds, the type on the right-hand side is the bound.
        // This bound may indirectly refer to type parameters from the type on
        // the left (e.g. through a required trait). As such we must expose
        // whatever values are assigned to such type parameters to the
        // right-hand side arguments.
        //
        // We do this by storing the assignment in the left-hand side arguments,
        // then expose those arguments as the right-hand side arguments. This
        // ensures that we de don't overwrite any assignments in the right-hand
        // side, as that could mess up type-checking (as these arguments are
        // provided by the user, instead of always being derived from the type
        // on the left).
        bounds.iter().all(|(&param, &bound)| {
            let val = env.left.get(param).unwrap();

            env.left.assign(bound, val);

            let mut env = env.with_left_as_right();
            let rules = Rules::new().with_relaxed_subtyping();

            if bound.is_mutable(self.db) && !val.allow_mutating(self.db) {
                return false;
            }

            if bound.is_copy(self.db) && !val.is_copy_type(self.db) {
                return false;
            }

            bound.requirements(self.db).into_iter().all(|r| {
                self.check_type_ref_with_trait(val, r, &mut env, rules)
            })
        })
    }

    pub fn type_compatible_with_bound(
        &mut self,
        left: TypeInstance,
        bound: TypeParameterId,
    ) -> bool {
        if bound.is_mutable(self.db)
            && !left.instance_of.allow_mutating(self.db)
        {
            return false;
        }

        if bound.is_copy(self.db) && !left.instance_of.is_copy_type(self.db) {
            return false;
        }

        bound
            .requirements(self.db)
            .into_iter()
            .all(|req| self.type_implements_trait(left, req))
    }

    fn check_type_ref(
        &mut self,
        left: TypeRef,
        right: TypeRef,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        if !self.checked.insert((left, right)) {
            return true;
        }

        // Resolve any assigned type parameters/placeholders to the types
        // they're assigned to.
        let left = self.resolve(left, &env.left, env.left_self, rules);
        let allow_ref = rules.implicit_root_ref;
        let allow_never = rules.allow_never;

        // We only apply the "infer as rigid" rule to the type on the left,
        // otherwise we may end up comparing e.g. a type instance to the rigid
        // type parameter on the right, which would always fail.
        //
        // This is OK because in practise, Any() only shows up on the left in
        // a select few cases.
        let rules = rules
            .dont_infer_as_rigid()
            .without_implicit_root_ref()
            .without_never();
        let orig_right = right;
        let right = self.resolve(right, &env.right, env.right_self, rules);

        // This indicates if the value on the left of the check is a value type
        // (e.g. Int or String).
        let is_val = left.is_value_type(self.db);
        let allow_mut = left.allow_mutating(self.db);

        // If at this point we encounter a type placeholder, it means the
        // placeholder is yet to be assigned a value.
        match left {
            // A `Never` can't be passed around because it, well, would never
            // happen. We allow the comparison so code such as `try else panic`
            // (where `panic` returns `Never`) is valid.
            TypeRef::Never if allow_never => match right {
                TypeRef::Placeholder(id) => {
                    id.assign_internal(self.db, left);
                    true
                }
                _ => true,
            },
            // Type errors are compatible with all other types to prevent a
            // cascade of type errors.
            TypeRef::Error => match right {
                TypeRef::Placeholder(id) => {
                    id.assign_internal(self.db, left);
                    true
                }
                _ => true,
            },
            TypeRef::Owned(left_id) => match right {
                TypeRef::Any(right_id) if !rules.kind.is_return() => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Owned(right_id) => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Mut(right_id) if allow_mut && allow_ref => {
                    let rules = rules.without_implicit_root_ref();

                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id) if is_val || allow_ref => {
                    let rules = rules.without_implicit_root_ref();

                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Uni(right_id) | TypeRef::UniRef(right_id)
                    if is_val =>
                {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    let allow = match id.ownership {
                        Ownership::Any | Ownership::Owned => true,
                        Ownership::Ref => is_val || allow_ref,
                        Ownership::Mut => allow_mut && allow_ref,
                        Ownership::Uni => is_val,
                        _ => false,
                    };

                    allow
                        && self.check_type_enum_with_placeholder(
                            left, left_id, orig_right, id, env, rules,
                        )
                }
                TypeRef::Pointer(_) if rules.kind.is_cast() => match left_id {
                    TypeEnum::TypeInstance(ins) => {
                        ins.instance_of().0 == INT_ID
                    }
                    TypeEnum::Foreign(ForeignType::Int(_, _)) => true,
                    _ => false,
                },
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Uni(left_id) => match right {
                TypeRef::Owned(right_id)
                    if rules.uni_compatible_with_owned || is_val =>
                {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Any(right_id) if !rules.kind.is_return() => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Uni(right_id) => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id) | TypeRef::Mut(right_id) if is_val => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    let allow = match id.ownership {
                        Ownership::Owned => {
                            rules.uni_compatible_with_owned || is_val
                        }
                        Ownership::Any | Ownership::Uni => true,
                        _ => false,
                    };

                    allow
                        && self.check_type_enum_with_placeholder(
                            left, left_id, orig_right, id, env, rules,
                        )
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Any(left_id) => match right {
                // Mut and Owned are not allowed because we don't know the
                // runtime ownership of our value. Ref is fine, because we can
                // always turn an Owned/Ref/Mut/etc into a Ref.
                TypeRef::Any(right_id) | TypeRef::Ref(right_id) => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    matches!(id.ownership, Ownership::Any | Ownership::Ref)
                        && self.check_type_enum_with_placeholder(
                            left, left_id, orig_right, id, env, rules,
                        )
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Ref(left_id) => match right {
                TypeRef::Any(TypeEnum::TypeParameter(pid))
                    if pid.is_mutable(self.db) && !is_val =>
                {
                    false
                }
                TypeRef::Any(right_id) if !rules.kind.is_return() => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id) => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Owned(right_id)
                | TypeRef::Uni(right_id)
                | TypeRef::UniRef(right_id)
                    if is_val =>
                {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    match id.ownership {
                        Ownership::Any | Ownership::Ref => {}
                        Ownership::Mut => return false,
                        _ if is_val => {}
                        _ => return false,
                    }

                    self.check_type_enum_with_placeholder(
                        left, left_id, orig_right, id, env, rules,
                    )
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Mut(left_id) => match right {
                TypeRef::Any(right_id) if !rules.kind.is_return() => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id) => {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Mut(right_id) => self.check_type_enum(
                    left_id,
                    right_id,
                    env,
                    rules.without_subtyping(),
                ),
                TypeRef::Owned(right_id)
                | TypeRef::Uni(right_id)
                | TypeRef::UniRef(right_id)
                | TypeRef::UniMut(right_id)
                    if is_val =>
                {
                    self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    let allow = match id.ownership {
                        Ownership::Any | Ownership::Ref | Ownership::Mut => {
                            true
                        }
                        _ => is_val,
                    };

                    allow
                        && self.check_type_enum_with_placeholder(
                            left, left_id, orig_right, id, env, rules,
                        )
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::UniRef(_) => right.is_error(self.db),
            TypeRef::UniMut(_) => right.is_error(self.db),
            TypeRef::Placeholder(left_id) => {
                use Ownership::*;

                let rval = right.is_value_type(self.db);
                let allow = match (left_id.ownership, right) {
                    (_, TypeRef::Error | TypeRef::Never) => true,
                    (exp, TypeRef::Placeholder(id)) => {
                        let res = match (exp, id.ownership) {
                            // If the placeholder on the left doesn't have an
                            // ownership requirement, it can safely be assigned
                            // the placeholder on the right, because in doing so
                            // we infer it as whatever type is assigned to the
                            // placeholder on the right.
                            (Any, _) => true,
                            (Owned, Owned | Any) => true,
                            (Uni, Owned) => rules.uni_compatible_with_owned,
                            (Uni, Uni | Any) => true,
                            (Ref, Any) => id
                                .required(self.db)
                                .map_or(true, |p| !p.is_mutable(self.db)),
                            (Ref, Ref) => true,
                            (Mut, Any | Ref | Mut) => true,
                            _ => false,
                        };

                        if res {
                            id.assign_internal(self.db, left);
                        }

                        res
                    }
                    (Any, _) => true,
                    (Owned, TypeRef::Any(_)) => !rules.kind.is_return(),
                    (Owned, TypeRef::Owned(_)) => true,
                    (Owned, TypeRef::Ref(_) | TypeRef::Mut(_)) => {
                        allow_ref || rval
                    }
                    (Uni, TypeRef::Owned(_)) => {
                        rules.uni_compatible_with_owned || rval
                    }
                    (Uni, TypeRef::Ref(_) | TypeRef::Mut(_)) => rval,
                    (Uni, TypeRef::Uni(_)) => true,
                    (Ref, TypeRef::Any(TypeEnum::TypeParameter(pid))) => {
                        !pid.is_mutable(self.db) || rval
                    }
                    (Ref, TypeRef::Any(_)) => !rules.kind.is_return(),
                    (Ref, TypeRef::Ref(_)) => true,
                    (
                        Ref,
                        TypeRef::Owned(_) | TypeRef::Uni(_) | TypeRef::Mut(_),
                    ) => rval,
                    (Mut, TypeRef::Any(_)) => !rules.kind.is_return(),
                    (Mut, TypeRef::Ref(_) | TypeRef::Mut(_)) => true,
                    (Mut, TypeRef::Owned(_) | TypeRef::Uni(_)) => rval,
                    (Pointer, TypeRef::Pointer(_)) => true,
                    _ => false,
                };

                if allow {
                    left_id.assign_internal(self.db, right);
                }

                allow
            }
            TypeRef::Pointer(left_id) => match right {
                TypeRef::Pointer(right_id) => {
                    rules.kind.is_cast()
                        || self.check_type_enum(left_id, right_id, env, rules)
                }
                TypeRef::Owned(TypeEnum::Foreign(ForeignType::Int(_, _))) => {
                    rules.kind.is_cast()
                }
                TypeRef::Owned(TypeEnum::TypeInstance(ins)) => {
                    rules.kind.is_cast() && ins.instance_of().0 == INT_ID
                }
                TypeRef::Placeholder(right_id) => {
                    match right_id.ownership {
                        Ownership::Any | Ownership::Pointer => {}
                        _ => return false,
                    }

                    self.check_type_enum_with_placeholder(
                        left, left_id, orig_right, right_id, env, rules,
                    )
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn check_type_enum(
        &mut self,
        left_id: TypeEnum,
        right_id: TypeEnum,
        env: &mut Environment,
        mut rules: Rules,
    ) -> bool {
        let trait_rules = rules;

        if let Subtyping::Strict = rules.subtyping {
            rules.subtyping = Subtyping::No;
        }

        match left_id {
            TypeEnum::Type(_) | TypeEnum::Trait(_) | TypeEnum::Module(_) => {
                // Classes, traits and modules themselves aren't treated as
                // types and thus can't be passed around, mostly because this
                // just isn't useful. To further reinforce this, these types
                // aren't compatible with anything.
                false
            }
            TypeEnum::TypeInstance(lhs) => match right_id {
                TypeEnum::TypeInstance(rhs) => {
                    if lhs.instance_of != rhs.instance_of {
                        if rules.kind.is_cast()
                            && lhs.instance_of.is_numeric()
                            && rhs.instance_of.is_numeric()
                        {
                            return true;
                        }

                        return false;
                    }

                    if !lhs.instance_of.is_generic(self.db) {
                        return true;
                    }

                    let lhs_args = lhs.type_arguments(self.db).unwrap();
                    let rhs_args = rhs.type_arguments(self.db).unwrap();

                    lhs.instance_of.type_parameters(self.db).into_iter().all(
                        |param| {
                            lhs_args.get(param).zip(rhs_args.get(param)).is_some_and(
                                |(lhs, rhs)| {
                                    self.check_type_ref(lhs, rhs, env, rules)
                                },
                            )
                        },
                    )
                }
                TypeEnum::TraitInstance(rhs) => {
                    if rules.kind.is_cast()
                        && (!lhs.instance_of().allow_cast_to_trait(self.db)
                            || !rhs.instance_of().is_cast_safe(self.db))
                    {
                        return false;
                    }

                    self.check_type_with_trait(lhs, rhs, env, trait_rules)
                }
                TypeEnum::TypeParameter(_) if rules.kind.is_cast() => false,
                TypeEnum::TypeParameter(rhs) => {
                    if rhs.is_copy(self.db)
                        && !lhs.instance_of().is_copy_type(self.db)
                    {
                        return false;
                    }

                    rhs.requirements(self.db).into_iter().all(|req| {
                        // One-time subtyping is enabled because we want to
                        // allow passing types to type parameters with
                        // requirements.
                        self.check_type_with_trait(
                            lhs,
                            req,
                            env,
                            rules.with_relaxed_subtyping(),
                        )
                    })
                }
                TypeEnum::Foreign(_) => {
                    rules.kind.is_cast()
                        && lhs.instance_of().allow_cast_to_foreign(self.db)
                }
                _ => false,
            },
            TypeEnum::TraitInstance(lhs) => match right_id {
                // Self in a trait isn't compatible with trait _values_ because
                // Self might not support dynamic dispatch.
                TypeEnum::TraitInstance(rhs) if lhs == rhs => true,
                TypeEnum::TraitInstance(_) if lhs.self_type => false,
                TypeEnum::TraitInstance(rhs) => {
                    self.check_traits(lhs, rhs, env, rules)
                }
                TypeEnum::TypeParameter(_) if rules.kind.is_cast() => false,
                TypeEnum::TypeParameter(rhs) if rhs.is_copy(self.db) => false,
                TypeEnum::TypeParameter(rhs) => rhs
                    .requirements(self.db)
                    .into_iter()
                    .all(|req| self.check_traits(lhs, req, env, rules)),
                _ => false,
            },
            TypeEnum::TypeParameter(lhs) => match right_id {
                TypeEnum::TypeParameter(rhs) => {
                    self.check_parameters(lhs, rhs, env, rules)
                }
                TypeEnum::Foreign(_) => rules.kind.is_cast(),
                // When comparing type arguments in e.g. field signatures, the
                // type on the right might be rigid while the type on the left
                // isn't. This is because the LHS type is also stored (e.g. in
                // method argument signatures), so they can't _always_ be rigid.
                TypeEnum::RigidTypeParameter(rhs) if env.left_rigid => {
                    self.check_parameters(lhs, rhs, env, rules)
                }
                _ => false,
            },
            TypeEnum::RigidTypeParameter(lhs)
            | TypeEnum::AtomicTypeParameter(lhs) => {
                self.check_rigid_with_type_enum(lhs, right_id, env, rules)
            }
            TypeEnum::Closure(lhs) => match right_id {
                TypeEnum::Closure(rhs) => {
                    let lhs_obj = lhs.get(self.db);
                    let rhs_obj = rhs.get(self.db);

                    self.check_arguments(
                        &lhs_obj.arguments,
                        &rhs_obj.arguments,
                        env,
                        rules,
                        false,
                    ) && self.check_type_ref(
                        lhs_obj.return_type,
                        rhs_obj.return_type,
                        env,
                        rules,
                    )
                }
                TypeEnum::TypeParameter(rhs)
                    if rhs.requirements(self.db).is_empty() =>
                {
                    // Closures can't implement traits, so they're only
                    // compatible with type parameters that don't have any
                    // requirements.
                    !rhs.is_copy(self.db)
                }
                _ => false,
            },
            TypeEnum::Foreign(ForeignType::Int(lsize, lsigned)) => {
                if rules.kind.is_cast() {
                    match right_id {
                        TypeEnum::Foreign(_) => true,
                        TypeEnum::TypeInstance(ins) => {
                            // 64-bits integers can be cast to Inko objects, as
                            // this is needed when interfacing with C.
                            matches!(ins.instance_of().0, INT_ID | FLOAT_ID)
                                || lsize == 64
                        }
                        _ => lsize == 64,
                    }
                } else {
                    match right_id {
                        TypeEnum::Foreign(ForeignType::Int(rsize, rsigned)) => {
                            lsize == rsize && lsigned == rsigned
                        }
                        _ => false,
                    }
                }
            }
            TypeEnum::Foreign(ForeignType::Float(lsize)) => {
                if rules.kind.is_cast() {
                    match right_id {
                        TypeEnum::Foreign(_) => true,
                        TypeEnum::TypeInstance(ins) => {
                            matches!(ins.instance_of().0, INT_ID | FLOAT_ID)
                        }
                        _ => false,
                    }
                } else {
                    match right_id {
                        TypeEnum::Foreign(ForeignType::Float(rsize)) => {
                            lsize == rsize
                        }
                        _ => false,
                    }
                }
            }
        }
    }

    fn check_rigid_with_type_enum(
        &mut self,
        left: TypeParameterId,
        right: TypeEnum,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        match right {
            TypeEnum::RigidTypeParameter(rhs) => left == rhs,
            TypeEnum::TypeParameter(rhs) => {
                if left == rhs {
                    return true;
                }

                if left.is_copy(self.db) != rhs.is_copy(self.db) {
                    return false;
                }

                rhs.requirements(self.db).into_iter().all(|req| {
                    self.check_parameter_with_trait(left, req, env, rules)
                })
            }
            TypeEnum::Foreign(_) => rules.kind.is_cast(),
            _ => false,
        }
    }

    fn check_type_enum_with_placeholder(
        &mut self,
        left: TypeRef,
        left_id: TypeEnum,
        original_right: TypeRef,
        placeholder: TypePlaceholderId,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        // By assigning the placeholder first, recursive checks against the same
        // placeholder don't keep recursing into this method, instead checking
        // against the value on the left.
        //
        // When comparing `ref A` with `ref B` or `mut A` with `mut B`, we want
        // to assign `B` to `A`, not `ref A`/`mut A`.
        if left.has_ownership(self.db)
            && (original_right.has_ownership(self.db)
                || placeholder.has_ownership())
        {
            // Any() is meant for type parameters _only_, i.e. Any(Int) makes no
            // sense. We don't use Owned() because Owned() isn't compatible with
            // Any() when using type parameters, which in turn would disallow
            // certain forms of type inference.
            let assign = if let TypeEnum::TypeParameter(_)
            | TypeEnum::RigidTypeParameter(_) = left_id
            {
                TypeRef::Any(left_id)
            } else {
                TypeRef::Owned(left_id)
            };

            placeholder.assign_internal(self.db, assign);
        } else {
            placeholder.assign_internal(self.db, left);
        }

        let req = if let Some(req) = placeholder.required(self.db) {
            req
        } else {
            return true;
        };

        if (req.is_mutable(self.db) && !left.allow_mutating(self.db))
            || (req.is_copy(self.db) && !left.is_copy_type(self.db))
        {
            placeholder.assign_internal(self.db, TypeRef::Unknown);
            return false;
        }

        let reqs = req.requirements(self.db);

        if reqs.is_empty() {
            return true;
        }

        // At this point no value is assigned yet, so it's safe to allow
        // sub-typing through traits.
        let rules = rules.with_relaxed_subtyping();
        let res = match left_id {
            TypeEnum::TypeInstance(lhs) => reqs
                .into_iter()
                .all(|req| self.check_type_with_trait(lhs, req, env, rules)),
            TypeEnum::TraitInstance(lhs) => reqs
                .into_iter()
                .all(|req| self.check_traits(lhs, req, env, rules)),
            TypeEnum::TypeParameter(lhs)
            | TypeEnum::RigidTypeParameter(lhs) => {
                reqs.into_iter().all(|req| {
                    self.check_parameter_with_trait(lhs, req, env, rules)
                })
            }
            _ => false,
        };

        // If we keep the assignment in case of a type error, formatted type
        // errors may be confusing as they would report the left-hand side as
        // the expected value, rather than the underlying type parameter.
        if !res {
            placeholder.assign_internal(self.db, TypeRef::Unknown);
        }

        res
    }

    pub fn type_implements_trait(
        &mut self,
        left: TypeInstance,
        right: TraitInstance,
    ) -> bool {
        let mut env = Environment::new(
            TypeArguments::for_type(self.db, left),
            TypeArguments::for_trait(self.db, right),
        );

        let rules = Rules::new().with_relaxed_subtyping();

        self.check_type_with_trait(left, right, &mut env, rules)
    }

    fn check_type_with_trait(
        &mut self,
        left: TypeInstance,
        right: TraitInstance,
        env: &mut Environment,
        mut rules: Rules,
    ) -> bool {
        // When checking trait implementations we don't know exactly how a `uni
        // T` value is used, and thus can't know if it's safe to compare it to a
        // `T`. Consider this example:
        //
        //     trait Equal[T] {
        //       fn ==(other: T) -> Bool
        //     }
        //
        //     type Thing {}
        //
        //     impl Equal[uni Thing] for Thing {
        //       fn ==(other: uni Thing) -> Bool {
        //         true
        //       }
        //     }
        //
        // If we end up comparing `Equal[uni Thing]` with `Equal[Thing]` we
        // can't allow this, because the argument of `==` could then be given a
        // `Thing` when we instead expect a `uni Thing`.
        rules.uni_compatible_with_owned = false;

        // `Array[Cat]` isn't compatible with `mut Array[Animal]`, as that could
        // result in a `Dog` being added to the Array.
        match rules.subtyping {
            Subtyping::No => return false,
            Subtyping::Relaxed => rules.subtyping = Subtyping::No,
            Subtyping::Strict => {
                if !left.instance_of.allow_cast_to_trait(self.db) {
                    return false;
                }

                rules.subtyping = Subtyping::No;
            }
        }

        let imp = if let Some(found) =
            left.instance_of.trait_implementation(self.db, right.instance_of)
        {
            found
        } else {
            return false;
        };

        if left.instance_of.is_generic(self.db) {
            // The implemented trait may refer to type parameters of the
            // implementing type, so we need to expose those using a new scope.
            let mut sub_scope = env.clone();

            left.type_arguments(self.db)
                .unwrap()
                .copy_into(&mut sub_scope.left);

            self.check_bounds(&imp.bounds, &mut sub_scope)
                && self.check_traits(imp.instance, right, &mut sub_scope, rules)
        } else {
            self.check_bounds(&imp.bounds, env)
                && self.check_traits(imp.instance, right, env, rules)
        }
    }

    fn check_type_ref_with_trait(
        &mut self,
        left: TypeRef,
        right: TraitInstance,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        match left {
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::UniRef(id)
            | TypeRef::UniMut(id)
            | TypeRef::Any(id) => match id {
                TypeEnum::TypeInstance(lhs) => {
                    self.check_type_with_trait(lhs, right, env, rules)
                }
                TypeEnum::TraitInstance(lhs) => {
                    self.check_traits(lhs, right, env, rules)
                }
                TypeEnum::TypeParameter(lhs)
                | TypeEnum::RigidTypeParameter(lhs) => {
                    self.check_parameter_with_trait(lhs, right, env, rules)
                }
                _ => false,
            },
            TypeRef::Placeholder(id) => match id.value(self.db) {
                Some(typ) => {
                    self.check_type_ref_with_trait(typ, right, env, rules)
                }
                // When the placeholder isn't assigned a value, the comparison
                // is treated as valid but we don't assign a type. This is
                // because in this scenario we can't reliably guess what the
                // type is, and what its ownership should be.
                _ => true,
            },
            TypeRef::Never => true,
            _ => false,
        }
    }

    fn check_parameter_with_trait(
        &mut self,
        left: TypeParameterId,
        right: TraitInstance,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        left.requirements(self.db)
            .into_iter()
            .any(|left| self.check_traits(left, right, env, rules))
    }

    fn check_parameters(
        &mut self,
        left: TypeParameterId,
        right: TypeParameterId,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        if left == right {
            return true;
        }

        if left.is_copy(self.db) != right.is_copy(self.db) {
            return false;
        }

        right
            .requirements(self.db)
            .into_iter()
            .all(|req| self.check_parameter_with_trait(left, req, env, rules))
    }

    fn check_traits(
        &mut self,
        left: TraitInstance,
        right: TraitInstance,
        env: &mut Environment,
        mut rules: Rules,
    ) -> bool {
        // Similar to when checking types with traits, we have to be more
        // strict about comparing `uni T` values with `T` values.
        rules.uni_compatible_with_owned = false;

        if left == right {
            return true;
        }

        if left.instance_of != right.instance_of {
            return left
                .instance_of
                .required_traits(self.db)
                .into_iter()
                .any(|lhs| self.check_traits(lhs, right, env, rules));
        }

        if !left.instance_of.is_generic(self.db) {
            return true;
        }

        let lhs_args = left.type_arguments(self.db).unwrap();
        let rhs_args = right.type_arguments(self.db).unwrap();

        left.instance_of.type_parameters(self.db).into_iter().all(|param| {
            lhs_args
                .get(param)
                .zip(rhs_args.get(param))
                .is_some_and(|(l, r)| {
                    self.check_type_ref(l, r, env, rules.infer_as_rigid())
                })
        })
    }

    fn check_arguments(
        &mut self,
        left: &Arguments,
        right: &Arguments,
        env: &mut Environment,
        rules: Rules,
        same_name: bool,
    ) -> bool {
        if left.len() != right.len() {
            return false;
        }

        left.mapping.values().zip(right.mapping.values()).all(
            |(ours, theirs)| {
                if same_name && ours.name != theirs.name {
                    return false;
                }

                self.check_type_ref(
                    ours.value_type,
                    theirs.value_type,
                    env,
                    rules,
                )
            },
        )
    }

    fn resolve(
        &self,
        typ: TypeRef,
        arguments: &TypeArguments,
        self_type: Option<TypeEnum>,
        rules: Rules,
    ) -> TypeRef {
        let mut result = match typ {
            TypeRef::Owned(TypeEnum::TypeParameter(id)) => {
                // Owned type parameters should only be assigned owned types.
                // This check ensures that if we have e.g. `move T` and
                // `T = ref User`, we don't turn that into `User`, as this could
                // allow certain invalid type-checks to pass. An example of that
                // is this:
                //
                //     trait Foo[T] {
                //       fn foo -> move T
                //     }
                //
                //     type Thing {}
                //
                //     impl Foo[ref Thing] for Thing {
                //       fn foo -> ref Thing {
                //         self
                //       }
                //     }
                //
                // Here `Thing.foo` should be invalid because `Foo.foo` mandates
                // the return type is owned, but `ref Thing` isn't. If we just
                // returned the resolved type as-is, we'd turn `move T` into
                // `ref Thing` and allow the implementation, which isn't
                // correct.
                //
                // We return `Unknown` here so we can guarantee the check fails,
                // as this type isn't compatible with anything.
                match self.resolve_type_parameter(
                    typ, id, arguments, self_type, rules,
                ) {
                    res @ TypeRef::Owned(_) => res,
                    TypeRef::Placeholder(id) => {
                        // We reach this point if the type parameter is assigned
                        // an unassigned placeholder.
                        TypeRef::Placeholder(id.as_owned())
                    }
                    _ => TypeRef::Unknown,
                }
            }
            TypeRef::Uni(TypeEnum::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, self_type, rules)
                .as_uni(self.db),
            TypeRef::Any(TypeEnum::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, self_type, rules),
            TypeRef::Ref(TypeEnum::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, self_type, rules)
                .as_ref(self.db),
            TypeRef::Mut(TypeEnum::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, self_type, rules)
                .as_mut(self.db),
            TypeRef::Pointer(TypeEnum::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, self_type, rules)
                .as_pointer(self.db),
            TypeRef::Placeholder(id) => id
                .value(self.db)
                .map_or(typ, |v| self.resolve(v, arguments, self_type, rules)),
            _ => typ,
        };

        if let Some(stype) = self_type {
            result = match result {
                TypeRef::Owned(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Owned(stype)
                }
                TypeRef::Ref(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Ref(stype)
                }
                TypeRef::Mut(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Mut(stype)
                }
                TypeRef::Uni(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Uni(stype)
                }
                TypeRef::UniRef(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::UniRef(stype)
                }
                TypeRef::UniMut(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::UniMut(stype)
                }
                TypeRef::Any(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Any(stype)
                }
                TypeRef::Pointer(TypeEnum::TraitInstance(i)) if i.self_type => {
                    TypeRef::Pointer(stype)
                }
                _ => result,
            };
        }

        if rules.rigid_parameters {
            result.as_rigid_type_parameter()
        } else {
            result
        }
    }

    fn resolve_type_parameter(
        &self,
        typ: TypeRef,
        id: TypeParameterId,
        arguments: &TypeArguments,
        self_type: Option<TypeEnum>,
        rules: Rules,
    ) -> TypeRef {
        match arguments.get(id.original(self.db).unwrap_or(id)) {
            Some(arg @ TypeRef::Placeholder(id)) => id
                .value(self.db)
                .map(|v| self.resolve(v, arguments, self_type, rules))
                .unwrap_or(arg),
            Some(arg) => arg,
            _ => typ,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::format_type;
    use crate::test::{
        any, closure, generic_instance, generic_trait_instance,
        generic_trait_instance_id, immutable, immutable_uni, implement,
        instance, mutable, mutable_uni, new_extern_type, new_parameter,
        new_trait, new_type, owned, parameter, placeholder, pointer, rigid,
        trait_instance, trait_instance_id, type_arguments, type_bounds, uni,
    };
    use crate::{
        Block, Closure, Location, ModuleId, Sign, TraitImplementation, Type,
        TypeId, TypeKind, TypePlaceholder, Visibility,
    };

    #[track_caller]
    fn check_ok(db: &Database, left: TypeRef, right: TypeRef) {
        if !TypeChecker::check(db, left, right) {
            panic!(
                "Expected {} to be compatible with {}",
                format_type(db, left),
                format_type(db, right)
            );
        }
    }

    #[track_caller]
    fn check_ok_cast(db: &Database, left: TypeRef, right: TypeRef) {
        if !TypeChecker::check_cast(db, left, right) {
            panic!(
                "Expected {} to be compatible with {}",
                format_type(db, left),
                format_type(db, right)
            );
        }
    }

    #[track_caller]
    fn check_ok_placeholder(
        db: &Database,
        left: TypePlaceholderId,
        right: TypeRef,
    ) {
        check_ok(db, placeholder(left), right);
        left.assign_internal(db, TypeRef::Unknown);
    }

    #[track_caller]
    fn check_err_placeholder(
        db: &Database,
        left: TypePlaceholderId,
        right: TypeRef,
    ) {
        check_err(db, placeholder(left), right);
        left.assign_internal(db, TypeRef::Unknown);
    }

    #[track_caller]
    fn check_err_cast(db: &Database, left: TypeRef, right: TypeRef) {
        if TypeChecker::check_cast(db, left, right) {
            panic!(
                "Expected {} not to be compatible with {}",
                format_type(db, left),
                format_type(db, right)
            );
        }
    }

    #[track_caller]
    fn check_err(db: &Database, left: TypeRef, right: TypeRef) {
        assert!(
            !TypeChecker::check(db, left, right),
            "Expected {} to not be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    #[track_caller]
    fn check_err_return(db: &Database, left: TypeRef, right: TypeRef) {
        let stype = instance(TypeId::int());

        assert!(
            !TypeChecker::check_return(db, left, right, stype),
            "Expected {} to not be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    #[test]
    fn test_never() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "T");
        let to_string = new_trait(&mut db, "ToString");
        let var1 = TypePlaceholder::alloc(&mut db, None);

        let var2 = TypePlaceholder::alloc(&mut db, Some(param));

        param.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_ok(&db, TypeRef::Never, placeholder(var1));
        check_ok(&db, TypeRef::Never, placeholder(var2));
        check_ok(&db, TypeRef::Never, TypeRef::Never);
    }

    #[test]
    fn test_owned_type_instance() {
        let mut db = Database::new();
        let foo = new_type(&mut db, "Foo");
        let bar = new_type(&mut db, "Bar");
        let int = TypeId::int();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let to_string = new_trait(&mut db, "ToString");
        let p1 = new_parameter(&mut db, "T");
        let p2 = new_parameter(&mut db, "X");
        let var2 = TypePlaceholder::alloc(&mut db, Some(p1));
        let var3 = TypePlaceholder::alloc(&mut db, Some(p1));
        let var4 = TypePlaceholder::alloc(&mut db, Some(p2));

        p2.set_copy(&mut db);
        p1.add_requirements(&mut db, vec![trait_instance(to_string)]);
        implement(&mut db, trait_instance(to_string), bar);

        check_ok(&db, owned(instance(foo)), owned(instance(foo)));
        check_ok(&db, owned(instance(foo)), any(instance(foo)));

        // This placeholder doesn't have any requirements
        check_ok(&db, owned(instance(foo)), placeholder(var1));
        assert_eq!(var1.value(&db), Some(owned(instance(foo))));

        // The placeholder is now assigned to Foo, so Bar shouldn't be
        // compatible with it.
        check_err(&db, owned(instance(bar)), placeholder(var1));

        // Foo doesn't implement ToString, so the check fails.
        check_err(&db, owned(instance(foo)), placeholder(var2));
        assert!(var2.value(&db).is_none());

        // Bar implements ToString, so this _does_ check and assigns the
        // placeholder.
        check_ok(&db, owned(instance(bar)), placeholder(var3));
        assert_eq!(var3.value(&db), Some(owned(instance(bar))));

        // Value types can be passed to a reference/unique values.
        check_ok(&db, owned(instance(int)), immutable(instance(int)));
        check_ok(&db, owned(instance(int)), uni(instance(int)));
        check_ok(&db, owned(instance(int)), immutable_uni(instance(int)));
        check_ok(&db, owned(instance(foo)), TypeRef::Error);
        check_ok(&db, owned(instance(int)), owned(parameter(p2)));

        check_err(&db, owned(instance(int)), mutable(instance(int)));
        check_err(&db, owned(instance(int)), mutable_uni(instance(int)));
        check_err(&db, owned(instance(foo)), immutable(instance(foo)));
        check_err(&db, owned(instance(foo)), mutable(instance(foo)));
        check_err(&db, owned(instance(foo)), owned(instance(bar)));
        check_err(&db, owned(instance(foo)), TypeRef::Never);
        check_err(&db, owned(instance(foo)), pointer(instance(foo)));

        check_err(&db, owned(instance(foo)), owned(parameter(p2)));
        check_err(&db, owned(instance(foo)), placeholder(var4));
        check_err(&db, owned(parameter(p1)), placeholder(var4));
        check_err(&db, uni(instance(foo)), uni(parameter(p2)));
        check_err(&db, mutable(instance(foo)), mutable(parameter(p2)));
        check_err(&db, immutable(instance(foo)), immutable(parameter(p2)));

        // Trait values are always on the heap.
        check_err(
            &db,
            owned(trait_instance_id(to_string)),
            owned(parameter(p2)),
        );
        check_err(&db, owned(trait_instance_id(to_string)), placeholder(var4));
        check_err(&db, uni(trait_instance_id(to_string)), uni(parameter(p2)));
        check_err(
            &db,
            mutable(trait_instance_id(to_string)),
            mutable(parameter(p2)),
        );
        check_err(
            &db,
            immutable(trait_instance_id(to_string)),
            immutable(parameter(p2)),
        );
    }

    #[test]
    fn test_extern_type_instance() {
        let mut db = Database::new();
        let foo = new_extern_type(&mut db, "Foo");
        let bar = new_extern_type(&mut db, "Bar");
        let param = new_parameter(&mut db, "T");

        check_ok(&db, owned(instance(foo)), owned(instance(foo)));

        check_err(&db, owned(instance(foo)), owned(instance(bar)));
        check_ok(&db, owned(instance(foo)), owned(parameter(param)));
        check_ok(&db, uni(instance(foo)), owned(parameter(param)));
    }

    #[test]
    fn test_owned_generic_type_instance() {
        let mut db = Database::new();
        let array = new_type(&mut db, "Array");
        let thing = new_type(&mut db, "Thing");
        let to_string = new_trait(&mut db, "ToString");
        let length = new_trait(&mut db, "Length");
        let equal = new_trait(&mut db, "Equal");

        equal.new_type_parameter(&mut db, "X".to_string());

        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let var = TypePlaceholder::alloc(&mut db, None);

        // V: Equal[V]
        let v_param = new_parameter(&mut db, "V");

        {
            let req = generic_trait_instance(
                &mut db,
                equal,
                vec![any(parameter(v_param))],
            );

            v_param.add_requirements(&mut db, vec![req]);
        }

        {
            let bound = new_parameter(&mut db, "Tbound");

            bound.add_requirements(&mut db, vec![trait_instance(to_string)]);

            let trait_impl = TraitImplementation {
                instance: trait_instance(to_string),
                bounds: type_bounds(vec![(array_param, bound)]),
            };

            // impl ToString for Array if T: ToString
            array.add_trait_implementation(&mut db, trait_impl);
        }

        // impl Length for Array
        array.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(length),
                bounds: TypeBounds::new(),
            },
        );

        // impl ToString for Thing
        thing.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(to_string),
                bounds: TypeBounds::new(),
            },
        );

        // impl Equal[Thing] for Thing
        {
            let eq = generic_trait_instance(
                &mut db,
                equal,
                vec![owned(instance(thing))],
            );

            thing.add_trait_implementation(
                &mut db,
                TraitImplementation { instance: eq, bounds: TypeBounds::new() },
            );
        }

        // impl Equal[Array[T]] for Array if T: Equal[T]
        {
            let bound = new_parameter(&mut db, "Tbound");
            let bound_eq = generic_trait_instance(
                &mut db,
                equal,
                vec![any(parameter(bound))],
            );

            bound.add_requirements(&mut db, vec![bound_eq]);

            let array_t = owned(generic_instance(
                &mut db,
                array,
                vec![any(parameter(bound))],
            ));

            let impl_ins =
                generic_trait_instance(&mut db, equal, vec![array_t]);
            let trait_impl = TraitImplementation {
                instance: impl_ins,
                bounds: type_bounds(vec![(array_param, bound)]),
            };

            array.add_trait_implementation(&mut db, trait_impl);
        }

        let things1 =
            generic_instance(&mut db, array, vec![owned(instance(thing))]);
        let things2 =
            generic_instance(&mut db, array, vec![owned(instance(thing))]);
        let thing_refs =
            generic_instance(&mut db, array, vec![immutable(instance(thing))]);
        let floats = generic_instance(&mut db, array, vec![TypeRef::float()]);
        let vars = generic_instance(&mut db, array, vec![placeholder(var)]);
        let eq_things =
            generic_trait_instance_id(&mut db, equal, vec![owned(things1)]);
        let things_empty = generic_instance(&mut db, array, Vec::new());

        check_ok(&db, owned(things1), owned(things1));
        check_ok(&db, owned(things1), owned(things2));
        check_ok(&db, owned(things1), any(parameter(v_param)));

        check_err(&db, owned(thing_refs), owned(parameter(v_param)));
        check_err(&db, owned(things1), owned(trait_instance_id(length)));
        check_err(&db, owned(floats), owned(trait_instance_id(length)));
        check_err(&db, owned(things1), owned(trait_instance_id(to_string)));
        check_err(&db, owned(vars), owned(trait_instance_id(to_string)));
        assert!(var.value(&db).is_none());

        check_err(&db, owned(things1), owned(eq_things));
        check_err(&db, owned(things1), owned(things_empty));
        check_err(&db, owned(things1), owned(floats));
        check_err(&db, owned(floats), owned(trait_instance_id(to_string)));
        check_err(&db, owned(floats), any(parameter(v_param)));
    }

    #[test]
    fn test_uni_type_instance() {
        let mut db = Database::new();
        let foo = new_type(&mut db, "Foo");
        let bar = new_type(&mut db, "Bar");
        let int = TypeId::int();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let to_string = new_trait(&mut db, "ToString");
        let param = new_parameter(&mut db, "T");
        let var2 = TypePlaceholder::alloc(&mut db, Some(param));
        let var3 = TypePlaceholder::alloc(&mut db, Some(param));

        param.add_requirements(&mut db, vec![trait_instance(to_string)]);
        implement(&mut db, trait_instance(to_string), bar);

        check_ok(&db, uni(instance(foo)), uni(instance(foo)));
        check_ok(&db, uni(instance(foo)), owned(instance(foo)));
        check_ok(&db, uni(instance(foo)), any(instance(foo)));

        // This placeholder doesn't have any requirements
        check_ok(&db, uni(instance(foo)), placeholder(var1));
        assert_eq!(var1.value(&db), Some(uni(instance(foo))));

        // The placeholder is now assigned to Foo, so Bar shouldn't be
        // compatible with it.
        check_err(&db, uni(instance(bar)), placeholder(var1));

        // Foo doesn't implement ToString, so the check fails.
        check_err(&db, uni(instance(foo)), placeholder(var2));
        assert!(var2.value(&db).is_none());

        // Bar implements ToString, so this _does_ check and assigns the
        // placeholder.
        check_ok(&db, uni(instance(bar)), placeholder(var3));
        assert_eq!(var3.value(&db), Some(uni(instance(bar))));

        // Value types can be passed to a reference.
        check_ok(&db, uni(instance(int)), immutable(instance(int)));
        check_ok(&db, uni(instance(int)), mutable(instance(int)));
        check_ok(&db, uni(instance(foo)), TypeRef::Error);
        check_err(&db, uni(instance(foo)), immutable(instance(foo)));
        check_err(&db, uni(instance(foo)), mutable(instance(foo)));
        check_err(&db, uni(instance(foo)), uni(instance(bar)));
        check_err(&db, uni(instance(foo)), TypeRef::Never);
    }

    #[test]
    fn test_uni_generic_type_instance() {
        let mut db = Database::new();
        let array = new_type(&mut db, "Array");
        let thing = new_type(&mut db, "Thing");
        let to_string = new_trait(&mut db, "ToString");
        let length = new_trait(&mut db, "Length");
        let equal = new_trait(&mut db, "Equal");

        equal.new_type_parameter(&mut db, "X".to_string());

        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let var = TypePlaceholder::alloc(&mut db, None);

        // V: Equal[V]
        let v_param = new_parameter(&mut db, "V");

        {
            let req = generic_trait_instance(
                &mut db,
                equal,
                vec![any(parameter(v_param))],
            );

            v_param.add_requirements(&mut db, vec![req]);
        }

        {
            let bound = new_parameter(&mut db, "Tbound");

            bound.add_requirements(&mut db, vec![trait_instance(to_string)]);

            let trait_impl = TraitImplementation {
                instance: trait_instance(to_string),
                bounds: type_bounds(vec![(array_param, bound)]),
            };

            // impl ToString for Array if T: ToString
            array.add_trait_implementation(&mut db, trait_impl);
        }

        // impl Length for Array
        array.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(length),
                bounds: TypeBounds::new(),
            },
        );

        // impl ToString for Thing
        thing.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(to_string),
                bounds: TypeBounds::new(),
            },
        );

        // impl Equal[uni Thing] for Thing
        {
            let eq = generic_trait_instance(
                &mut db,
                equal,
                vec![uni(instance(thing))],
            );

            thing.add_trait_implementation(
                &mut db,
                TraitImplementation { instance: eq, bounds: TypeBounds::new() },
            );
        }

        // impl Equal[uni Array[T]] for Array if T: Equal[T]
        {
            let bound = new_parameter(&mut db, "Tbound");
            let bound_eq = generic_trait_instance(
                &mut db,
                equal,
                vec![any(parameter(bound))],
            );

            bound.add_requirements(&mut db, vec![bound_eq]);

            let array_t = uni(generic_instance(
                &mut db,
                array,
                vec![any(parameter(bound))],
            ));

            let impl_ins =
                generic_trait_instance(&mut db, equal, vec![array_t]);
            let trait_impl = TraitImplementation {
                instance: impl_ins,
                bounds: type_bounds(vec![(array_param, bound)]),
            };

            array.add_trait_implementation(&mut db, trait_impl);
        }

        let things1 =
            generic_instance(&mut db, array, vec![uni(instance(thing))]);
        let things2 =
            generic_instance(&mut db, array, vec![uni(instance(thing))]);
        let thing_refs =
            generic_instance(&mut db, array, vec![immutable(instance(thing))]);
        let floats = generic_instance(&mut db, array, vec![TypeRef::float()]);
        let vars = generic_instance(&mut db, array, vec![placeholder(var)]);
        let eq_things =
            generic_trait_instance_id(&mut db, equal, vec![uni(things1)]);

        check_ok(&db, uni(things1), uni(things1));
        check_ok(&db, uni(things1), uni(things2));
        check_ok(&db, uni(things1), any(parameter(v_param)));
        check_ok(&db, uni(things1), uni(parameter(v_param)));

        check_err(&db, uni(things1), uni(eq_things));
        check_err(&db, uni(things1), uni(trait_instance_id(length)));
        check_err(&db, uni(floats), uni(trait_instance_id(length)));
        check_err(&db, uni(things1), uni(trait_instance_id(to_string)));
        check_err(&db, uni(vars), uni(trait_instance_id(to_string)));
        assert!(var.value(&db).is_none());
        check_err(&db, uni(thing_refs), uni(parameter(v_param)));
        check_err(&db, uni(things1), uni(floats));
        check_err(&db, uni(floats), uni(trait_instance_id(to_string)));
        check_err(&db, uni(floats), any(parameter(v_param)));
    }

    #[test]
    fn test_infer() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, any(parameter(param1)), any(parameter(param2)));
        check_ok(&db, any(parameter(param1)), immutable(parameter(param2)));
        check_ok(&db, any(parameter(param1)), TypeRef::Error);
        check_ok(&db, any(parameter(param1)), placeholder(var));
        assert_eq!(var.value(&db), Some(any(parameter(param1))));

        check_err(&db, any(parameter(param1)), owned(parameter(param2)));
        check_err(&db, any(parameter(param1)), uni(parameter(param2)));
        check_err(&db, any(parameter(param1)), mutable(parameter(param2)));
    }

    #[test]
    fn test_ref() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let int = TypeId::int();
        let var = TypePlaceholder::alloc(&mut db, None);
        let param = new_parameter(&mut db, "T");
        let mutable_var = TypePlaceholder::alloc(&mut db, Some(param));

        param.set_mutable(&mut db);

        check_ok(&db, immutable(instance(thing)), immutable(instance(thing)));
        check_ok(&db, immutable(instance(thing)), any(instance(thing)));

        // Value types can be passed around this way.
        check_ok(&db, immutable(instance(int)), owned(instance(int)));
        check_ok(&db, immutable(instance(int)), uni(instance(int)));

        check_ok(&db, immutable(instance(thing)), placeholder(var));
        assert_eq!(var.value(&db), Some(immutable(instance(thing))));

        check_ok(&db, immutable(instance(thing)), TypeRef::Error);
        check_ok(&db, immutable(instance(int)), any(parameter(param)));

        check_err(&db, immutable(instance(int)), placeholder(mutable_var));
        check_err(&db, immutable(instance(int)), mutable(instance(int)));
        check_err(&db, immutable(instance(thing)), mutable(instance(thing)));
        check_err(&db, immutable(instance(thing)), owned(instance(thing)));
        check_err(&db, immutable(instance(thing)), any(parameter(param)));
        check_err(&db, immutable(instance(thing)), placeholder(mutable_var));
        check_err(&db, immutable(instance(thing)), pointer(instance(thing)));
    }

    #[test]
    fn test_mut() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let int = TypeId::int();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, mutable(instance(thing)), immutable(instance(thing)));
        check_ok(&db, mutable(instance(thing)), mutable(instance(thing)));
        check_ok(&db, mutable(instance(thing)), any(instance(thing)));

        // Value types can be passed around this way.
        check_ok(&db, mutable(instance(int)), owned(instance(int)));
        check_ok(&db, mutable(instance(int)), uni(instance(int)));

        check_ok(&db, mutable(instance(thing)), placeholder(var));
        assert_eq!(var.value(&db), Some(mutable(instance(thing))));

        check_ok(&db, mutable(instance(thing)), TypeRef::Error);

        check_err(&db, mutable(instance(thing)), owned(instance(thing)));
        check_err(&db, mutable(instance(thing)), uni(instance(thing)));
        check_err(&db, mutable(instance(thing)), pointer(instance(thing)));
    }

    #[test]
    fn test_mut_with_mut_type_parameter() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None);
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );

        let res = TypeChecker::new(&db).run(
            mutable(rigid(param)),
            mutable(parameter(param)),
            &mut env,
        );

        assert!(res);
        assert_eq!(var.value(&db), Some(any(rigid(param))));
    }

    #[test]
    fn test_ref_instance_with_ref_type_parameter() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None);
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );

        let res = TypeChecker::new(&db).run(
            immutable(instance(thing)),
            immutable(parameter(param)),
            &mut env,
        );

        assert!(res);
        assert_eq!(var.value(&db), Some(owned(instance(thing))));
    }

    #[test]
    fn test_owned_instance_with_pointer_type_parameter() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None);
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );

        let res = TypeChecker::new(&db).run(
            owned(instance(thing)),
            pointer(parameter(param)),
            &mut env,
        );

        assert!(!res);
    }

    #[test]
    fn test_pointer_instance_with_pointer_type_parameter() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None);
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );

        let res = TypeChecker::new(&db).run(
            pointer(instance(thing)),
            pointer(parameter(param)),
            &mut env,
        );

        assert!(res);
        assert_eq!(var.value(&db), Some(owned(instance(thing))));
    }

    #[test]
    fn test_ref_instance_with_type_parameter_with_ref_ownership() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None).as_ref();
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );
        let res = TypeChecker::new(&db).run(
            immutable(instance(thing)),
            any(parameter(param)),
            &mut env,
        );

        assert!(res);
        assert_eq!(var.value(&db), Some(immutable(instance(thing))));
        assert_eq!(var.get(&db).value.get(), owned(instance(thing)));
    }

    #[test]
    fn test_pointer_instance_with_type_parameter_with_pointer_ownership() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, None).as_pointer();
        let mut env = Environment::new(
            TypeArguments::new(),
            type_arguments(vec![(param, placeholder(var))]),
        );
        let res = TypeChecker::new(&db).run(
            pointer(instance(thing)),
            any(parameter(param)),
            &mut env,
        );

        assert!(res);
        assert_eq!(var.value(&db), Some(pointer(instance(thing))));
        assert_eq!(var.get(&db).value.get(), owned(instance(thing)));
    }

    #[test]
    fn test_mut_with_mut_placeholder_with_requirements() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "T");
        let to_foo = new_trait(&mut db, "ToFoo");
        let array = TypeId::array();
        let var = TypePlaceholder::alloc(&mut db, Some(param));

        array.new_type_parameter(&mut db, "T".to_string());
        param.add_requirements(&mut db, vec![trait_instance(to_foo)]);
        TypeId::int().add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(to_foo),
                bounds: TypeBounds::new(),
            },
        );

        let given =
            mutable(generic_instance(&mut db, array, vec![TypeRef::int()]));

        let exp =
            mutable(generic_instance(&mut db, array, vec![placeholder(var)]));

        check_ok(&db, given, exp);
    }

    #[test]
    fn test_uni_ref() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, immutable_uni(instance(thing)), TypeRef::Error);
        check_err(
            &db,
            immutable_uni(instance(thing)),
            immutable(instance(thing)),
        );
        check_err(
            &db,
            immutable_uni(instance(thing)),
            immutable_uni(instance(thing)),
        );
        check_err(
            &db,
            immutable_uni(instance(thing)),
            mutable_uni(instance(thing)),
        );
        check_err(&db, immutable_uni(instance(thing)), placeholder(var));
    }

    #[test]
    fn test_uni_mut() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, mutable_uni(instance(thing)), TypeRef::Error);
        check_err(
            &db,
            mutable_uni(instance(thing)),
            mutable_uni(instance(thing)),
        );
        check_err(
            &db,
            mutable_uni(instance(thing)),
            immutable_uni(instance(thing)),
        );
        check_err(&db, mutable_uni(instance(thing)), mutable(instance(thing)));
        check_err(&db, mutable_uni(instance(thing)), placeholder(var));
    }

    #[test]
    fn test_placeholder() {
        let mut db = Database::new();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, placeholder(var), TypeRef::int());
        assert_eq!(var.value(&db), Some(TypeRef::int()));
    }

    #[test]
    fn test_placeholder_with_ownership() {
        let mut db = Database::new();
        let int = TypeId::int();
        let thing = new_type(&mut db, "Thing");
        let any_var = TypePlaceholder::alloc(&mut db, None);
        let owned_var = TypePlaceholder::alloc(&mut db, None).as_owned();
        let ref_var = TypePlaceholder::alloc(&mut db, None).as_ref();
        let mut_var = TypePlaceholder::alloc(&mut db, None).as_mut();
        let uni_var = TypePlaceholder::alloc(&mut db, None).as_uni();

        check_ok(&db, owned(instance(thing)), placeholder(any_var));
        check_ok(&db, owned(instance(thing)), placeholder(owned_var));

        check_err(&db, owned(instance(thing)), placeholder(ref_var));
        check_err(&db, owned(instance(thing)), placeholder(mut_var));
        check_err(&db, owned(instance(thing)), placeholder(uni_var));

        check_ok(&db, owned(instance(int)), placeholder(ref_var));
        check_ok(&db, owned(instance(int)), placeholder(uni_var));
        check_err(&db, owned(instance(int)), placeholder(mut_var));
    }

    #[test]
    fn test_placeholder_with_placeholder() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "T");

        param.set_mutable(&mut db);

        let p1 = TypePlaceholder::alloc(&mut db, None);
        let p2 = TypePlaceholder::alloc(&mut db, None);
        let p3 = TypePlaceholder::alloc(&mut db, Some(param));

        check_ok_placeholder(&db, p1, placeholder(p2));
        check_ok_placeholder(&db, p1, placeholder(p2.as_owned()));
        check_ok_placeholder(&db, p1.as_owned(), placeholder(p2));
        check_ok_placeholder(&db, p1.as_owned(), placeholder(p2.as_owned()));
        check_ok_placeholder(&db, p1.as_uni(), placeholder(p2));
        check_ok_placeholder(&db, p1.as_uni(), placeholder(p2.as_owned()));
        check_ok_placeholder(&db, p1.as_uni(), placeholder(p2.as_uni()));
        check_ok_placeholder(&db, p1.as_ref(), placeholder(p2));
        check_err_placeholder(&db, p1.as_ref(), placeholder(p3));
        check_ok_placeholder(&db, p1.as_ref(), placeholder(p2.as_ref()));
        check_ok_placeholder(&db, p1.as_mut(), placeholder(p2));
        check_ok_placeholder(&db, p1.as_mut(), placeholder(p2.as_ref()));
        check_ok_placeholder(&db, p1.as_mut(), placeholder(p2.as_mut()));
    }

    #[test]
    fn test_placeholder_with_type() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "T");
        let param2 = new_parameter(&mut db, "T");

        param2.set_mutable(&mut db);

        let p1 = TypePlaceholder::alloc(&mut db, None);
        let int = TypeId::int();
        let thing = new_type(&mut db, "Thing");

        check_ok_placeholder(&db, p1, owned(instance(int)));
        check_ok_placeholder(&db, p1.as_owned(), owned(instance(int)));
        check_ok_placeholder(&db, p1.as_owned(), any(instance(int)));
        check_ok_placeholder(&db, p1.as_uni(), owned(instance(int)));
        check_ok_placeholder(&db, p1.as_uni(), immutable(instance(int)));
        check_ok_placeholder(&db, p1.as_uni(), mutable(instance(int)));
        check_ok_placeholder(&db, p1.as_uni(), uni(instance(int)));
        check_ok_placeholder(&db, p1.as_ref(), any(parameter(param1)));
        check_ok_placeholder(&db, p1.as_ref(), immutable(instance(int)));
        check_ok_placeholder(&db, p1.as_ref(), any(instance(int)));
        check_ok_placeholder(&db, p1.as_ref(), owned(instance(int)));
        check_ok_placeholder(&db, p1.as_ref(), uni(instance(int)));
        check_ok_placeholder(&db, p1.as_ref(), mutable(instance(int)));
        check_ok_placeholder(&db, p1.as_mut(), any(instance(int)));
        check_ok_placeholder(&db, p1.as_mut(), immutable(instance(int)));
        check_ok_placeholder(&db, p1.as_mut(), mutable(instance(int)));
        check_ok_placeholder(&db, p1.as_mut(), owned(instance(int)));
        check_ok_placeholder(&db, p1.as_mut(), uni(instance(int)));
        check_ok_placeholder(&db, p1.as_uni(), owned(instance(thing)));

        check_err_placeholder(&db, p1.as_uni(), immutable(instance(thing)));
        check_err_placeholder(&db, p1.as_uni(), mutable(instance(thing)));
        check_err_placeholder(&db, p1.as_ref(), any(parameter(param2)));
        check_err_placeholder(&db, p1.as_ref(), owned(instance(thing)));
    }

    #[test]
    fn test_placeholder_with_placeholder_assigns_both() {
        let mut db = Database::new();
        let p1 = TypePlaceholder::alloc(&mut db, None);
        let p2 = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, placeholder(p1), placeholder(p2));

        assert_eq!(p1.raw_value(&db), placeholder(p2));
        assert_eq!(p2.raw_value(&db), placeholder(p1));
    }

    #[test]
    fn test_pointer_with_placeholder() {
        let mut db = Database::new();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let var2 = TypePlaceholder::alloc(&mut db, None);
        let int_ptr = pointer(instance(TypeId::int()));

        check_ok(&db, placeholder(var1), int_ptr);
        check_ok(&db, int_ptr, placeholder(var2));
        assert_eq!(var1.value(&db), Some(int_ptr));
        assert_eq!(var2.value(&db), Some(int_ptr));
    }

    #[test]
    fn test_struct_with_placeholder() {
        let mut db = Database::new();
        let typ = new_extern_type(&mut db, "A");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, owned(instance(typ)), placeholder(var));
    }

    #[test]
    fn test_pointer_with_rigid_parameter() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");

        check_ok(&db, pointer(rigid(param1)), pointer(rigid(param1)));
        check_err(&db, pointer(rigid(param1)), pointer(rigid(param2)));
        check_err(&db, pointer(parameter(param1)), pointer(rigid(param1)));
    }

    #[test]
    fn test_type_with_trait() {
        let mut db = Database::new();
        let animal = new_trait(&mut db, "Animal");
        let cat = new_type(&mut db, "Cat");
        let array = TypeId::array();

        array.new_type_parameter(&mut db, "T".to_string());
        implement(&mut db, trait_instance(animal), cat);

        // Array[Cat]
        let cats =
            owned(generic_instance(&mut db, array, vec![owned(instance(cat))]));

        // ref Array[Cat]
        let ref_cats = immutable(generic_instance(
            &mut db,
            array,
            vec![owned(instance(cat))],
        ));

        // mut Array[Cat]
        let mut_cats = mutable(generic_instance(
            &mut db,
            array,
            vec![owned(instance(cat))],
        ));

        // Array[Animal]
        let animals = owned(generic_instance(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        // ref Array[Animal]
        let ref_animals = immutable(generic_instance(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        // mut Array[Animal]
        let mut_animals = mutable(generic_instance(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        check_ok(&db, animals, animals);
        check_err(&db, cats, animals);
        check_err(&db, ref_cats, ref_animals);
        check_err(&db, mut_cats, ref_animals);
        check_err(&db, mut_cats, mut_animals);
    }

    #[test]
    fn test_traits() {
        let mut db = Database::new();
        let to_string = new_trait(&mut db, "ToString");
        let display = new_trait(&mut db, "Display");
        let debug = new_trait(&mut db, "Debug");

        display.add_required_trait(&mut db, trait_instance(to_string));
        debug.add_required_trait(&mut db, trait_instance(display));

        check_ok(
            &db,
            owned(trait_instance_id(to_string)),
            owned(trait_instance_id(to_string)),
        );
        check_ok(
            &db,
            owned(trait_instance_id(display)),
            owned(trait_instance_id(to_string)),
        );
        check_ok(
            &db,
            owned(trait_instance_id(debug)),
            owned(trait_instance_id(to_string)),
        );
        check_err(
            &db,
            owned(trait_instance_id(to_string)),
            owned(trait_instance_id(display)),
        );
    }

    #[test]
    fn test_generic_traits() {
        let mut db = Database::new();
        let equal = new_trait(&mut db, "Equal");
        let thing = new_type(&mut db, "Thing");

        equal.new_type_parameter(&mut db, "T".to_string());

        // Equal[Thing]
        let eq_thing = owned(generic_trait_instance_id(
            &mut db,
            equal,
            vec![owned(instance(thing))],
        ));
        let eq_ref_thing = owned(generic_trait_instance_id(
            &mut db,
            equal,
            vec![immutable(instance(thing))],
        ));
        let eq_mut_thing = owned(generic_trait_instance_id(
            &mut db,
            equal,
            vec![mutable(instance(thing))],
        ));

        let eq_empty =
            owned(generic_trait_instance_id(&mut db, equal, Vec::new()));

        check_ok(&db, eq_thing, eq_thing);
        check_ok(&db, eq_ref_thing, eq_ref_thing);
        check_ok(&db, eq_mut_thing, eq_mut_thing);
        check_err(&db, eq_thing, eq_ref_thing);
        check_err(&db, eq_thing, eq_mut_thing);
        check_err(&db, eq_thing, eq_empty);
    }

    #[test]
    fn test_type_parameter_with_trait() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let param3 = new_parameter(&mut db, "C");
        let equal = new_trait(&mut db, "Equal");
        let to_string = new_trait(&mut db, "ToString");

        param1.add_requirements(&mut db, vec![trait_instance(equal)]);
        param3.add_requirements(&mut db, vec![trait_instance(equal)]);
        param3.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_err(
            &db,
            owned(parameter(param1)),
            owned(trait_instance_id(equal)),
        );
        check_err(
            &db,
            owned(parameter(param3)),
            owned(trait_instance_id(equal)),
        );
        check_err(
            &db,
            owned(parameter(param3)),
            owned(trait_instance_id(to_string)),
        );
        check_err(
            &db,
            owned(parameter(param2)),
            owned(trait_instance_id(equal)),
        );
    }

    #[test]
    fn test_trait_with_parameter() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let equal = new_trait(&mut db, "Equal");
        let foo = new_trait(&mut db, "Foo");
        let to_string = new_trait(&mut db, "ToString");

        foo.add_required_trait(&mut db, trait_instance(equal));
        foo.add_required_trait(&mut db, trait_instance(to_string));

        param1.add_requirements(&mut db, vec![trait_instance(equal)]);
        param2.add_requirements(&mut db, vec![trait_instance(equal)]);
        param2.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_ok(
            &db,
            owned(trait_instance_id(equal)),
            owned(parameter(param1)),
        );
        check_ok(&db, owned(trait_instance_id(foo)), owned(parameter(param2)));
        check_err(
            &db,
            owned(trait_instance_id(to_string)),
            owned(parameter(param1)),
        );
    }

    #[test]
    fn test_parameters() {
        let mut db = Database::new();
        let p1 = new_parameter(&mut db, "A");
        let p2 = new_parameter(&mut db, "B");
        let p3 = new_parameter(&mut db, "C");
        let p4 = new_parameter(&mut db, "D");
        let p5 = new_parameter(&mut db, "E");
        let p6 = new_parameter(&mut db, "F");
        let equal = new_trait(&mut db, "Equal");
        let test = new_trait(&mut db, "Test");

        test.add_required_trait(&mut db, trait_instance(equal));
        p3.add_requirements(&mut db, vec![trait_instance(equal)]);
        p4.add_requirements(&mut db, vec![trait_instance(test)]);
        p5.set_copy(&mut db);
        p6.set_copy(&mut db);

        check_ok(&db, owned(parameter(p1)), owned(parameter(p2)));
        check_ok(&db, owned(parameter(p4)), owned(parameter(p3)));
        check_ok(&db, owned(parameter(p5)), owned(parameter(p6)));

        check_err(&db, owned(parameter(p3)), owned(parameter(p4)));
        check_err(&db, owned(parameter(p1)), owned(parameter(p5)));
        check_err(&db, owned(parameter(p5)), owned(parameter(p1)));
    }

    #[test]
    fn test_type_parameter_ref_assigned_to_owned() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "A");
        let thing = new_type(&mut db, "Thing");
        let args = type_arguments(vec![(param, owned(instance(thing)))]);
        let mut env = Environment::new(args.clone(), args);
        let res = TypeChecker::new(&db).run(
            immutable(instance(thing)),
            immutable(parameter(param)),
            &mut env,
        );

        assert!(res);
    }

    #[test]
    fn test_rigid() {
        let mut db = Database::new();
        let p1 = new_parameter(&mut db, "A");
        let p2 = new_parameter(&mut db, "B");
        let p3 = new_parameter(&mut db, "C");
        let p4 = new_parameter(&mut db, "C");
        let var = TypePlaceholder::alloc(&mut db, None);

        p3.set_copy(&mut db);

        check_ok(&db, owned(rigid(p1)), TypeRef::Error);
        check_ok(&db, immutable(rigid(p1)), immutable(rigid(p1)));
        check_ok(&db, owned(rigid(p1)), owned(rigid(p1)));
        check_ok(&db, owned(rigid(p1)), any(rigid(p1)));
        check_ok(&db, owned(rigid(p1)), any(parameter(p1)));
        check_ok(&db, immutable(rigid(p1)), immutable(parameter(p1)));
        check_ok(&db, owned(rigid(p1)), owned(parameter(p1)));
        check_ok(&db, owned(rigid(p3)), owned(rigid(p3)));

        check_ok(&db, owned(rigid(p1)), placeholder(var));
        assert_eq!(var.value(&db), Some(owned(rigid(p1))));

        check_err(&db, owned(rigid(p1)), owned(rigid(p2)));
        check_err(&db, owned(rigid(p1)), owned(rigid(p3)));
        check_err(&db, owned(rigid(p3)), owned(rigid(p1)));
        check_err(&db, owned(rigid(p3)), owned(parameter(p4)));
        check_err(&db, immutable(rigid(p1)), immutable(rigid(p2)));
    }

    #[test]
    fn test_rigid_with_trait() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let to_string = new_trait(&mut db, "ToString");
        let equal = new_trait(&mut db, "Equal");

        param1.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_err(
            &db,
            immutable(rigid(param1)),
            immutable(trait_instance_id(to_string)),
        );
        check_ok(&db, owned(rigid(param1)), any(parameter(param2)));

        // A doesn't implement Equal
        check_err(
            &db,
            immutable(rigid(param1)),
            immutable(trait_instance_id(equal)),
        );
    }

    #[test]
    fn test_simple_closures() {
        let mut db = Database::new();
        let fun1 = Closure::alloc(&mut db, false);
        let fun2 = Closure::alloc(&mut db, false);

        fun1.set_return_type(&mut db, TypeRef::int());
        fun2.set_return_type(&mut db, TypeRef::int());

        check_ok(&db, owned(closure(fun1)), owned(closure(fun2)));
    }

    #[test]
    fn test_closures_with_arguments() {
        let mut db = Database::new();
        let fun1 = Closure::alloc(&mut db, false);
        let fun2 = Closure::alloc(&mut db, false);
        let fun3 = Closure::alloc(&mut db, false);
        let fun4 = Closure::alloc(&mut db, false);
        let int = TypeRef::int();
        let float = TypeRef::float();
        let loc = Location::default();

        fun1.new_argument(&mut db, "a".to_string(), int, int, loc);
        fun2.new_argument(&mut db, "b".to_string(), int, int, loc);
        fun4.new_argument(&mut db, "a".to_string(), float, float, loc);
        fun1.set_return_type(&mut db, TypeRef::int());
        fun2.set_return_type(&mut db, TypeRef::int());
        fun3.set_return_type(&mut db, TypeRef::int());
        fun4.set_return_type(&mut db, TypeRef::int());

        check_ok(&db, owned(closure(fun1)), owned(closure(fun2)));
        check_err(&db, owned(closure(fun3)), owned(closure(fun2)));
        check_err(&db, owned(closure(fun1)), owned(closure(fun4)));
    }

    #[test]
    fn test_closures_with_return_types() {
        let mut db = Database::new();
        let fun1 = Closure::alloc(&mut db, false);
        let fun2 = Closure::alloc(&mut db, false);
        let fun3 = Closure::alloc(&mut db, false);
        let int = TypeRef::int();
        let float = TypeRef::float();

        fun1.set_return_type(&mut db, int);
        fun2.set_return_type(&mut db, int);
        fun3.set_return_type(&mut db, float);

        check_ok(&db, owned(closure(fun1)), owned(closure(fun2)));
        check_err(&db, owned(closure(fun1)), owned(closure(fun3)));
    }

    #[test]
    fn test_closure_with_parameter() {
        let mut db = Database::new();
        let fun = Closure::alloc(&mut db, false);
        let equal = new_trait(&mut db, "Equal");
        let p1 = new_parameter(&mut db, "A");
        let p2 = new_parameter(&mut db, "B");
        let p3 = new_parameter(&mut db, "C");

        p2.add_requirements(&mut db, vec![trait_instance(equal)]);
        p3.set_copy(&mut db);

        check_ok(&db, owned(closure(fun)), owned(parameter(p1)));
        check_err(&db, owned(closure(fun)), owned(parameter(p2)));
        check_err(&db, owned(closure(fun)), owned(parameter(p3)));
    }

    #[test]
    fn test_closure_with_placeholder() {
        let mut db = Database::new();
        let fun = Closure::alloc(&mut db, false);
        let param = new_parameter(&mut db, "A");
        let var = TypePlaceholder::alloc(&mut db, Some(param));

        check_ok(&db, owned(closure(fun)), placeholder(var));
    }

    #[test]
    fn test_recursive_type() {
        let mut db = Database::new();
        let array = TypeId::array();
        let var = TypePlaceholder::alloc(&mut db, None);

        array.new_type_parameter(&mut db, "T".to_string());

        let given =
            owned(generic_instance(&mut db, array, vec![placeholder(var)]));
        let ints =
            owned(generic_instance(&mut db, array, vec![TypeRef::int()]));
        let exp = owned(generic_instance(&mut db, array, vec![ints]));

        var.assign(&mut db, given);
        check_err(&db, given, exp);
    }

    #[test]
    fn test_mutable_bounds() {
        let mut db = Database::new();
        let array = TypeId::array();
        let thing = new_type(&mut db, "Thing");
        let update = new_trait(&mut db, "Update");
        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let array_bounds = new_parameter(&mut db, "T");
        let exp_param = new_parameter(&mut db, "Expected");

        exp_param.add_requirements(&mut db, vec![trait_instance(update)]);
        array_bounds.set_mutable(&mut db);
        array.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(update),
                bounds: type_bounds(vec![(array_param, array_bounds)]),
            },
        );

        // Array[Thing]
        let owned_things = owned(generic_instance(
            &mut db,
            array,
            vec![owned(instance(thing))],
        ));

        // Array[ref Thing]
        let ref_things = owned(generic_instance(
            &mut db,
            array,
            vec![immutable(instance(thing))],
        ));

        check_ok(&db, owned_things, owned(parameter(exp_param)));

        // `ref Thing` isn't mutable, so this check should fail.
        check_err(&db, ref_things, owned(parameter(exp_param)));
    }

    #[test]
    fn test_copy_bounds() {
        let mut db = Database::new();
        let array = TypeId::array();
        let heap = new_type(&mut db, "Heap");
        let stack = new_type(&mut db, "Stack");

        stack.set_copy_storage(&mut db);

        let update = new_trait(&mut db, "Update");
        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let array_bounds = new_parameter(&mut db, "T");
        let exp_param = new_parameter(&mut db, "Expected");

        exp_param.add_requirements(&mut db, vec![trait_instance(update)]);
        array_bounds.set_copy(&mut db);
        array.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(update),
                bounds: type_bounds(vec![(array_param, array_bounds)]),
            },
        );

        let stack_ary = owned(generic_instance(
            &mut db,
            array,
            vec![owned(instance(stack))],
        ));

        let heap_ary = owned(generic_instance(
            &mut db,
            array,
            vec![owned(instance(heap))],
        ));

        check_ok(&db, stack_ary, owned(parameter(exp_param)));
        check_err(&db, heap_ary, owned(parameter(exp_param)));
    }

    #[test]
    fn test_array_of_generic_types_with_traits() {
        let mut db = Database::new();
        let iter = new_trait(&mut db, "Iter");
        let array = TypeId::array();

        array.new_type_parameter(&mut db, "ArrayT".to_string());
        iter.new_type_parameter(&mut db, "IterT".to_string());

        let iterator = new_type(&mut db, "Iterator");
        let iterator_param =
            iterator.new_type_parameter(&mut db, "IteratorT".to_string());

        // impl Iter[T] for Iterator
        let iter_impl = TraitImplementation {
            instance: generic_trait_instance(
                &mut db,
                iter,
                vec![any(parameter(iterator_param))],
            ),
            bounds: TypeBounds::new(),
        };

        iterator.add_trait_implementation(&mut db, iter_impl);

        let int_iterator =
            owned(generic_instance(&mut db, iterator, vec![TypeRef::int()]));

        let int_iter = owned(generic_trait_instance_id(
            &mut db,
            iter,
            vec![TypeRef::int()],
        ));

        // Array[Iterator[Int]]
        let lhs = owned(generic_instance(&mut db, array, vec![int_iterator]));

        // Array[Iter[Int]]
        let rhs = owned(generic_instance(&mut db, array, vec![int_iter]));

        check_err(&db, lhs, rhs);
    }

    #[test]
    fn test_rigid_type_parameter() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let args = type_arguments(vec![(param, owned(instance(thing)))]);
        let mut env = Environment::new(args.clone(), args);
        let res = TypeChecker::new(&db).run(
            owned(instance(thing)),
            owned(rigid(param)),
            &mut env,
        );

        assert!(!res);
        check_ok(&db, owned(rigid(param)), any(parameter(param)));
    }

    #[test]
    fn test_rigid_type_parameter_with_requirements_with_placeholder() {
        let mut db = Database::new();
        let equal = new_trait(&mut db, "Equal");
        let param1 = new_parameter(&mut db, "P1");
        let param2 = new_parameter(&mut db, "P2");
        let var = TypePlaceholder::alloc(&mut db, Some(param2));

        equal.new_type_parameter(&mut db, "EQ".to_string());

        let param1_req = generic_trait_instance(
            &mut db,
            equal,
            vec![any(parameter(param1))],
        );

        let param2_req = generic_trait_instance(
            &mut db,
            equal,
            vec![any(parameter(param2))],
        );

        param1.add_requirements(&mut db, vec![param1_req]);
        param2.add_requirements(&mut db, vec![param2_req]);

        let args = type_arguments(vec![(param2, placeholder(var))]);
        let mut env = Environment::new(TypeArguments::new(), args);
        let res = TypeChecker::new(&db).run(
            owned(rigid(param1)),
            any(parameter(param2)),
            &mut env,
        );

        assert!(!res);
    }

    #[test]
    fn test_check_argument_with_mut() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let to_string = new_trait(&mut db, "ToString");

        thing.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(to_string),
                bounds: TypeBounds::new(),
            },
        );

        let mut env =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        assert!(!TypeChecker::new(&db).check_argument(
            mutable(instance(thing)),
            mutable(trait_instance_id(to_string)),
            &mut env,
        ));
    }

    #[test]
    fn test_check_argument_with_ref() {
        let mut db = Database::new();
        let array = TypeId::array();
        let int = TypeId::int();
        let to_string = new_trait(&mut db, "ToString");

        array.new_type_parameter(&mut db, "T".to_string());

        int.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(to_string),
                bounds: TypeBounds::new(),
            },
        );

        let mut env =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        let to_string_array = generic_instance(
            &mut db,
            array,
            vec![owned(trait_instance_id(to_string))],
        );

        let int_array =
            generic_instance(&mut db, array, vec![owned(instance(int))]);

        assert!(!TypeChecker::new(&db).check_argument(
            immutable(int_array),
            immutable(to_string_array),
            &mut env,
        ));
    }

    #[test]
    fn test_check_foreign_types() {
        let mut db = Database::new();
        let foo = Type::alloc(
            &mut db,
            "foo".to_string(),
            TypeKind::Extern,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        let bar = Type::alloc(
            &mut db,
            "bar".to_string(),
            TypeKind::Extern,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        let param = new_parameter(&mut db, "T");

        check_ok(
            &db,
            TypeRef::foreign_signed_int(8),
            TypeRef::foreign_signed_int(8),
        );
        check_ok(&db, TypeRef::foreign_float(32), TypeRef::foreign_float(32));
        check_ok(&db, owned(instance(foo)), owned(instance(foo)));

        check_ok_cast(
            &db,
            TypeRef::foreign_signed_int(8),
            TypeRef::foreign_signed_int(16),
        );

        check_ok_cast(
            &db,
            TypeRef::foreign_float(32),
            TypeRef::foreign_float(64),
        );

        check_ok_cast(&db, TypeRef::foreign_signed_int(32), TypeRef::int());
        check_ok_cast(&db, TypeRef::foreign_signed_int(64), TypeRef::string());
        check_ok_cast(
            &db,
            TypeRef::foreign_signed_int(64),
            owned(parameter(param)),
        );

        check_ok_cast(&db, TypeRef::string(), TypeRef::foreign_signed_int(64));
        check_ok_cast(
            &db,
            owned(parameter(param)),
            TypeRef::foreign_signed_int(64),
        );

        check_ok_cast(&db, TypeRef::foreign_float(32), TypeRef::int());
        check_ok_cast(&db, TypeRef::int(), TypeRef::foreign_signed_int(8));
        check_ok_cast(&db, TypeRef::float(), TypeRef::foreign_float(32));
        check_ok_cast(&db, TypeRef::float(), TypeRef::int());
        check_ok_cast(&db, TypeRef::int(), TypeRef::float());
        check_ok_cast(
            &db,
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Signed,
            ))),
            TypeRef::foreign_signed_int(8),
        );
        check_ok_cast(
            &db,
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Unsigned,
            ))),
            TypeRef::foreign_signed_int(8),
        );
        check_ok_cast(
            &db,
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Signed,
            ))),
            TypeRef::int(),
        );
        check_ok_cast(
            &db,
            TypeRef::int(),
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Signed,
            ))),
        );
        check_ok_cast(
            &db,
            TypeRef::foreign_signed_int(8),
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Signed,
            ))),
        );
        check_ok_cast(
            &db,
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                8,
                Sign::Signed,
            ))),
            TypeRef::pointer(TypeEnum::Foreign(ForeignType::Float(32))),
        );

        check_err(
            &db,
            TypeRef::foreign_signed_int(32),
            TypeRef::foreign_signed_int(8),
        );
        check_err(
            &db,
            TypeRef::foreign_signed_int(8),
            TypeRef::foreign_signed_int(16),
        );
        check_err(&db, TypeRef::foreign_float(32), TypeRef::foreign_float(64));
        check_err(
            &db,
            TypeRef::foreign_signed_int(8),
            TypeRef::foreign_float(32),
        );
        check_err(
            &db,
            TypeRef::foreign_float(8),
            TypeRef::foreign_signed_int(32),
        );
        check_err(&db, owned(instance(foo)), owned(instance(bar)));
        check_err(
            &db,
            owned(instance(foo)),
            TypeRef::pointer(TypeEnum::TypeInstance(TypeInstance::new(foo))),
        );
        check_err(
            &db,
            TypeRef::pointer(TypeEnum::TypeInstance(TypeInstance::new(foo))),
            TypeRef::pointer(TypeEnum::TypeInstance(TypeInstance::new(bar))),
        );
    }

    #[test]
    fn test_invalid_casts() {
        let mut db = Database::new();
        let to_string = new_trait(&mut db, "ToString");
        let param = new_parameter(&mut db, "T");
        let stack = new_type(&mut db, "Stack");

        stack.set_copy_storage(&mut db);

        for typ in [
            TypeId::int(),
            TypeId::float(),
            TypeId::boolean(),
            TypeId::nil(),
            TypeId::string(),
        ] {
            typ.add_trait_implementation(
                &mut db,
                TraitImplementation {
                    instance: trait_instance(to_string),
                    bounds: TypeBounds::new(),
                },
            );
        }

        let to_string_ins = owned(trait_instance_id(to_string));

        check_err_cast(&db, TypeRef::int(), to_string_ins);
        check_err_cast(&db, TypeRef::float(), to_string_ins);
        check_err_cast(&db, TypeRef::boolean(), to_string_ins);
        check_err_cast(&db, TypeRef::nil(), to_string_ins);
        check_err_cast(&db, TypeRef::string(), to_string_ins);
        check_err_cast(&db, TypeRef::int(), owned(parameter(param)));
        check_err_cast(&db, to_string_ins, owned(parameter(param)));
        check_err_cast(
            &db,
            owned(instance(stack)),
            owned(TypeEnum::Foreign(ForeignType::Int(32, Sign::Signed))),
        );
    }

    #[test]
    fn test_ref_value_type_with_uni_reference() {
        let db = Database::new();
        let int = TypeId::int();

        check_ok(&db, immutable(instance(int)), immutable_uni(instance(int)));
        check_ok(&db, mutable(instance(int)), mutable_uni(instance(int)));
    }

    #[test]
    fn test_check_ref_against_owned_parameter_with_assigned_type() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let mut env =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        env.right.assign(param, immutable(instance(thing)));
        assert!(!TypeChecker::new(&db).check_argument(
            immutable(instance(thing)),
            owned(parameter(param)),
            &mut env,
        ));
    }

    #[test]
    fn test_check_ref_against_owned_parameter_with_assigned_placeholder() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let var = TypePlaceholder::alloc(&mut db, None);
        let param = new_parameter(&mut db, "T");
        let mut env1 =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        let mut env2 =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        env1.right.assign(param, placeholder(var));
        env2.right.assign(param, placeholder(var));

        assert!(TypeChecker::new(&db).check_argument(
            owned(instance(thing)),
            owned(parameter(param)),
            &mut env1,
        ));

        assert!(!TypeChecker::new(&db).check_argument(
            immutable(instance(thing)),
            owned(parameter(param)),
            &mut env1,
        ));
    }

    #[test]
    fn test_check_owned_against_uni_placeholder() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, Some(param));
        let mut env =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        env.right.assign(param, placeholder(var));
        assert!(!TypeChecker::new(&db).check_argument(
            owned(instance(thing)),
            uni(parameter(param)),
            &mut env,
        ));
    }

    #[test]
    fn test_check_bounded_type_parameter() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let bound = new_parameter(&mut db, "T");

        bound.set_original(&mut db, param);

        let mut env =
            Environment::new(TypeArguments::new(), TypeArguments::new());

        env.left.assign(param, owned(instance(thing)));

        assert!(TypeChecker::new(&db).run(
            any(parameter(bound)),
            owned(instance(thing)),
            &mut env
        ));
    }

    #[test]
    fn test_check_return() {
        let mut db = Database::new();
        let thing = new_type(&mut db, "Thing");
        let owned_var = TypePlaceholder::alloc(&mut db, None).as_owned();
        let uni_var = TypePlaceholder::alloc(&mut db, None).as_uni();
        let ref_var = TypePlaceholder::alloc(&mut db, None).as_ref();
        let mut_var = TypePlaceholder::alloc(&mut db, None).as_mut();

        check_err_return(&db, owned(instance(thing)), any(instance(thing)));
        check_err_return(&db, uni(instance(thing)), any(instance(thing)));
        check_err_return(&db, immutable(instance(thing)), any(instance(thing)));
        check_err_return(&db, mutable(instance(thing)), any(instance(thing)));
        check_err_return(&db, placeholder(owned_var), any(instance(thing)));
        check_err_return(&db, placeholder(uni_var), any(instance(thing)));
        check_err_return(&db, placeholder(ref_var), any(instance(thing)));
        check_err_return(&db, placeholder(mut_var), any(instance(thing)));
    }
}
