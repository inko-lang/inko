//! Type checking of types.
use crate::{
    Arguments, ClassInstance, Database, MethodId, TraitInstance, TypeArguments,
    TypeBounds, TypeId, TypeParameterId, TypePlaceholderId, TypeRef,
};
use std::collections::HashSet;

#[derive(Copy, Clone)]
enum TraitSubtyping {
    /// Trait subtyping isn't allowed.
    No,

    /// Trait subtyping is allowed.
    Yes,

    /// Trait subtyping is allowed, but only for the first check.
    Once,
}

impl TraitSubtyping {
    fn allowed(self) -> bool {
        matches!(self, TraitSubtyping::Yes | TraitSubtyping::Once)
    }
}

#[derive(Copy, Clone)]
struct Rules {
    /// When set to `true`, subtyping of types through traits is allowed.
    subtyping: TraitSubtyping,

    /// When set to `true`, owned types can be type checked against reference
    /// types.
    relaxed_ownership: bool,

    /// When encountering an Infer() type, turn it into a rigid type.
    infer_as_rigid: bool,
}

impl Rules {
    fn new() -> Rules {
        Rules {
            subtyping: TraitSubtyping::Yes,
            relaxed_ownership: false,
            infer_as_rigid: false,
        }
    }

    fn no_subtyping(mut self) -> Rules {
        if let TraitSubtyping::Yes = self.subtyping {
            self.subtyping = TraitSubtyping::No
        }

        self
    }

    fn infer_as_rigid(mut self) -> Rules {
        self.infer_as_rigid = true;
        self
    }

    fn dont_infer_as_rigid(mut self) -> Rules {
        self.infer_as_rigid = false;
        self
    }

    fn relaxed(mut self) -> Rules {
        self.relaxed_ownership = true;
        self
    }

    fn strict(mut self) -> Rules {
        self.relaxed_ownership = false;
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
        Environment { left: left_arguments, right: right_arguments }
    }

    fn with_left_as_right(&self) -> Environment {
        Environment { left: self.left.clone(), right: self.left.clone() }
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

    pub fn check_argument(
        mut self,
        left_original: TypeRef,
        left: TypeRef,
        right: TypeRef,
        env: &mut Environment,
    ) -> bool {
        let mut rules = Rules::new();

        // If we pass an owned value to a mut, we'll allow comparing with a
        // trait but _only_ at the top (i.e `Cat -> mut Animal` is fine, but
        // `Cat -> mut Array[Animal]` isn't).
        if left_original.is_owned_or_uni(self.db) && right.is_mut(self.db) {
            rules.subtyping = TraitSubtyping::Once;
        }

        self.check_type_ref(left, right, env, rules)
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

        let param_rules = rules.no_subtyping();

        lhs.type_parameters
            .values()
            .iter()
            .zip(rhs.type_parameters.values().iter())
            .all(|(&lhs, &rhs)| {
                self.check_parameters(lhs, rhs, env, param_rules)
            })
            && self.check_arguments(
                &lhs.arguments,
                &rhs.arguments,
                env,
                rules,
                true,
            )
            && self.check_type_ref(lhs.return_type, rhs.return_type, env, rules)
    }

    pub fn check_bounds(
        &mut self,
        bounds: &TypeBounds,
        env: &mut Environment,
    ) -> bool {
        let rules = Rules::new();

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
            let rules = rules.relaxed();

            if bound.is_mutable(self.db) && !val.is_mutable(self.db) {
                return false;
            }

            bound.requirements(self.db).into_iter().all(|r| {
                self.check_type_ref_with_trait(val, r, &mut env, rules)
            })
        })
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

        // Relaxed ownership only applies to the check we perform below, not any
        // sub checks. The approach we take here means we only need to "reset"
        // this once for the sub checks.
        let relaxed = rules.relaxed_ownership;
        let rules = rules.strict();

        // Resolve any assigned type parameters/placeholders to the types
        // they're assigned to.
        let left = self.resolve(left, &env.left, rules);

        // We only apply the "infer as rigid" rule to the type on the left,
        // otherwise we may end up comparing e.g. a class instance to the rigid
        // type parameter on the right, which would always fail.
        //
        // This is OK because in practise, Infer() only shows up on the left in
        // a select few cases.
        let rules = rules.dont_infer_as_rigid();
        let original_right = right;
        let right = self.resolve(right, &env.right, rules);

        // If at this point we encounter a type placeholder, it means the
        // placeholder is yet to be assigned a value.
        match left {
            TypeRef::Any => match right {
                TypeRef::Any | TypeRef::Error => true,
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    id.required(self.db)
                        .map_or(true, |p| p.requirements(self.db).is_empty())
                }
                _ => false,
            },
            // A `Never` can't be passed around because it, well, would never
            // happen. We allow the comparison so code such as `try else panic`
            // (where `panic` returns `Never`) is valid.
            TypeRef::Never => match right {
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    true
                }
                _ => true,
            },
            // Type errors are compatible with all other types to prevent a
            // cascade of type errors.
            TypeRef::Error => match right {
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    true
                }
                _ => true,
            },
            // Rigid values are more restrictive when it comes to ownership, as
            // at compile-time we can't always know the exact ownership (i.e.
            // the parameter may be a ref at runtime).
            TypeRef::Owned(left_id @ TypeId::RigidTypeParameter(lhs)) => {
                match right {
                    TypeRef::Owned(TypeId::RigidTypeParameter(rhs)) => {
                        lhs == rhs
                    }
                    TypeRef::Infer(right_id) => {
                        self.check_rigid_with_type_id(lhs, right_id, env, rules)
                    }
                    TypeRef::Placeholder(id) => self
                        .check_type_id_with_placeholder(
                            left,
                            left_id,
                            original_right,
                            id,
                            env,
                            rules,
                        ),
                    TypeRef::Any | TypeRef::Error => true,
                    _ => false,
                }
            }
            TypeRef::Owned(left_id) => match right {
                TypeRef::Owned(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id)
                | TypeRef::Mut(right_id)
                | TypeRef::Uni(right_id)
                    if left.is_value_type(self.db) || relaxed =>
                {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(
                        left,
                        left_id,
                        original_right,
                        id,
                        env,
                        rules,
                    ),
                TypeRef::Any => {
                    if let TypeId::ClassInstance(ins) = left_id {
                        if ins.instance_of().kind(self.db).is_extern() {
                            return false;
                        }
                    }

                    true
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Uni(left_id) => match right {
                TypeRef::Owned(right_id)
                | TypeRef::Infer(right_id)
                | TypeRef::Uni(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Ref(right_id) | TypeRef::Mut(right_id)
                    if left.is_value_type(self.db) =>
                {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(
                        left,
                        left_id,
                        original_right,
                        id,
                        env,
                        rules,
                    ),
                TypeRef::Any => {
                    if let TypeId::ClassInstance(ins) = left_id {
                        if ins.instance_of().kind(self.db).is_extern() {
                            return false;
                        }
                    }

                    true
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Infer(left_id) => match right {
                // Mut and Owned are not allowed because we don't know the
                // runtime ownership of our value. Ref is fine, because we can
                // always turn an Owned/Ref/Mut/etc into a Ref.
                TypeRef::Infer(right_id) | TypeRef::Ref(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(
                        left,
                        left_id,
                        original_right,
                        id,
                        env,
                        rules,
                    ),
                TypeRef::Any | TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Ref(left_id) => match right {
                TypeRef::Infer(TypeId::TypeParameter(pid))
                    if pid.is_mutable(self.db)
                        && !left.is_value_type(self.db) =>
                {
                    false
                }
                TypeRef::Ref(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Owned(right_id)
                | TypeRef::Uni(right_id)
                | TypeRef::Mut(right_id)
                    if left.is_value_type(self.db) =>
                {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => {
                    if let Some(req) = id.required(self.db) {
                        if req.is_mutable(self.db)
                            && !left.is_value_type(self.db)
                        {
                            return false;
                        }
                    }

                    self.check_type_id_with_placeholder(
                        left,
                        left_id,
                        original_right,
                        id,
                        env,
                        rules,
                    )
                }
                TypeRef::Any | TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Mut(left_id) => match right {
                TypeRef::Ref(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Mut(right_id) => self.check_type_id(
                    left_id,
                    right_id,
                    env,
                    rules.no_subtyping(),
                ),
                TypeRef::Owned(right_id) | TypeRef::Uni(right_id)
                    if left.is_value_type(self.db) =>
                {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(
                        left,
                        left_id,
                        original_right,
                        id,
                        env,
                        rules,
                    ),
                TypeRef::Any | TypeRef::Error => true,
                _ => false,
            },
            TypeRef::RefUni(left_id) => match right {
                TypeRef::RefUni(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::MutUni(left_id) => match right {
                TypeRef::MutUni(right_id) => {
                    self.check_type_id(left_id, right_id, env, rules)
                }
                TypeRef::Error => true,
                _ => false,
            },
            TypeRef::Placeholder(left_id) => {
                // If we reach this point it means the placeholder isn't
                // assigned a value.
                left_id.assign(self.db, right);
                true
            }
            _ => false,
        }
    }

    fn check_type_id(
        &mut self,
        left_id: TypeId,
        right_id: TypeId,
        env: &mut Environment,
        mut rules: Rules,
    ) -> bool {
        // When sub-typing is allowed once (as is done when moving owned
        // arguments into trait references), this one-time exception only
        // applies when comparing a class against a trait. Here we disable the
        // rule for every thing else in one go, so it's more difficult to
        // accidentally apply the wrong rules for other comparisons.
        let trait_rules = rules;

        if let TraitSubtyping::Once = rules.subtyping {
            rules.subtyping = TraitSubtyping::No;
        }

        match left_id {
            TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_) => {
                // Classes, traits and modules themselves aren't treated as
                // types and thus can't be passed around, mostly because this
                // just isn't useful. To further reinforce this, these types
                // aren't compatible with anything.
                false
            }
            TypeId::ClassInstance(lhs) => match right_id {
                TypeId::ClassInstance(rhs) => {
                    if lhs.instance_of != rhs.instance_of {
                        return false;
                    }

                    if !lhs.instance_of.is_generic(self.db) {
                        return true;
                    }

                    let lhs_args = lhs.type_arguments(self.db);
                    let rhs_args = rhs.type_arguments(self.db);

                    lhs.instance_of.type_parameters(self.db).into_iter().all(
                        |param| {
                            lhs_args.get(param).zip(rhs_args.get(param)).map_or(
                                false,
                                |(lhs, rhs)| {
                                    self.check_type_ref(lhs, rhs, env, rules)
                                },
                            )
                        },
                    )
                }
                TypeId::TraitInstance(rhs)
                    if !lhs.instance_of().kind(self.db).is_extern() =>
                {
                    self.check_class_with_trait(lhs, rhs, env, trait_rules)
                }
                TypeId::TypeParameter(rhs)
                    if !lhs.instance_of().kind(self.db).is_extern() =>
                {
                    rhs.requirements(self.db).into_iter().all(|req| {
                        self.check_class_with_trait(lhs, req, env, rules)
                    })
                }
                _ => false,
            },
            TypeId::TraitInstance(lhs) => match right_id {
                TypeId::TraitInstance(rhs) => {
                    self.check_traits(lhs, rhs, env, rules)
                }
                TypeId::TypeParameter(rhs) => rhs
                    .requirements(self.db)
                    .into_iter()
                    .all(|req| self.check_traits(lhs, req, env, rules)),
                _ => false,
            },
            TypeId::TypeParameter(lhs) => match right_id {
                TypeId::TraitInstance(rhs) => {
                    self.check_parameter_with_trait(lhs, rhs, env, rules)
                }
                TypeId::TypeParameter(rhs) => {
                    self.check_parameters(lhs, rhs, env, rules)
                }
                _ => false,
            },
            TypeId::RigidTypeParameter(lhs) => {
                self.check_rigid_with_type_id(lhs, right_id, env, rules)
            }
            TypeId::Closure(lhs) => match right_id {
                TypeId::Closure(rhs) => {
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
                TypeId::TypeParameter(rhs)
                    if rhs.requirements(self.db).is_empty() =>
                {
                    // Closures can't implement traits, so they're only
                    // compatible with type parameters that don't have any
                    // requirements.
                    true
                }
                _ => false,
            },
        }
    }

    fn check_rigid_with_type_id(
        &mut self,
        left: TypeParameterId,
        right: TypeId,
        env: &mut Environment,
        rules: Rules,
    ) -> bool {
        match right {
            TypeId::RigidTypeParameter(rhs) => left == rhs,
            TypeId::TraitInstance(rhs) => {
                self.check_parameter_with_trait(left, rhs, env, rules)
            }
            TypeId::TypeParameter(rhs) => {
                if left == rhs {
                    return true;
                }

                rhs.requirements(self.db).into_iter().all(|req| {
                    self.check_parameter_with_trait(left, req, env, rules)
                })
            }
            _ => false,
        }
    }

    fn check_type_id_with_placeholder(
        &mut self,
        left: TypeRef,
        left_id: TypeId,
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
        if left.is_ref_or_mut(self.db) && original_right.is_ref_or_mut(self.db)
        {
            placeholder.assign(self.db, TypeRef::Owned(left_id));
        } else {
            placeholder.assign(self.db, left);
        }

        let req = if let Some(req) = placeholder.required(self.db) {
            req
        } else {
            return true;
        };

        let reqs = req.requirements(self.db);

        if reqs.is_empty() {
            return true;
        }

        let res = match left_id {
            TypeId::ClassInstance(lhs) => reqs
                .into_iter()
                .all(|req| self.check_class_with_trait(lhs, req, env, rules)),
            TypeId::TraitInstance(lhs) => reqs
                .into_iter()
                .all(|req| self.check_traits(lhs, req, env, rules)),
            TypeId::TypeParameter(lhs) | TypeId::RigidTypeParameter(lhs) => {
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
            placeholder.assign(self.db, TypeRef::Unknown);
        }

        res
    }

    pub fn class_implements_trait(
        &mut self,
        left: ClassInstance,
        right: TraitInstance,
    ) -> bool {
        let mut env = Environment::new(
            TypeArguments::for_class(self.db, left),
            TypeArguments::for_trait(self.db, right),
        );

        self.check_class_with_trait(left, right, &mut env, Rules::new())
    }

    fn check_class_with_trait(
        &mut self,
        left: ClassInstance,
        right: TraitInstance,
        env: &mut Environment,
        mut rules: Rules,
    ) -> bool {
        // `Array[Cat]` isn't compatible with `mut Array[Animal]`, as that could
        // result in a `Dog` being added to the Array.
        match rules.subtyping {
            TraitSubtyping::No => return false,
            TraitSubtyping::Yes => {}
            TraitSubtyping::Once => {
                rules.subtyping = TraitSubtyping::No;
            }
        }

        let imp = if let Some(found) =
            left.instance_of.trait_implementation(self.db, right.instance_of)
        {
            found
        } else {
            return false;
        };

        // Trait implementations may be over owned values (e.g.
        // `impl Equal[Foo] for Foo`). We allow such implementations to be
        // compatible with those over references (e.g. `Equal[ref Foo]`), as
        // otherwise the type system becomes to painful to work with.
        //
        // The reason this is sound is as follows:
        //
        // In the trait's methods, we can only refer to conrete types (in which
        // case the type arguments of the implementation are irrelevant), or to
        // the type parameters (if any) of the trait. These type parameters
        // either have their ownership inferred, or use whatever ownership is
        // explicitly provided.
        //
        // If any arguments need e.g. a reference then this is handled at the
        // call site. If a reference needs to be produced (e.g. as the return
        // value), it's up to the implementation of the method to do so.
        // References in turn can be created from both owned values and other
        // references.
        let rules = rules.relaxed();

        if left.instance_of.is_generic(self.db) {
            // The implemented trait may refer to type parameters of the
            // implementing class, so we need to expose those using a new scope.
            let mut sub_scope = env.clone();

            left.type_arguments(self.db).copy_into(&mut sub_scope.left);

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
            | TypeRef::RefUni(id)
            | TypeRef::MutUni(id)
            | TypeRef::Infer(id) => match id {
                TypeId::ClassInstance(lhs) => {
                    self.check_class_with_trait(lhs, right, env, rules)
                }
                TypeId::TraitInstance(lhs) => {
                    self.check_traits(lhs, right, env, rules)
                }
                TypeId::TypeParameter(lhs)
                | TypeId::RigidTypeParameter(lhs) => {
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
        rules: Rules,
    ) -> bool {
        if left == right {
            return true;
        }

        if left.instance_of != right.instance_of {
            return if rules.subtyping.allowed() {
                left.instance_of
                    .required_traits(self.db)
                    .into_iter()
                    .any(|lhs| self.check_traits(lhs, right, env, rules))
            } else {
                false
            };
        }

        if !left.instance_of.is_generic(self.db) {
            return true;
        }

        let lhs_args = left.type_arguments(self.db);
        let rhs_args = right.type_arguments(self.db);

        left.instance_of.type_parameters(self.db).into_iter().all(|param| {
            let rules = rules.infer_as_rigid();

            lhs_args
                .get(param)
                .zip(rhs_args.get(param))
                .map_or(false, |(lhs, rhs)| {
                    self.check_type_ref(lhs, rhs, env, rules)
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

        left.mapping.values().iter().zip(right.mapping.values().iter()).all(
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
        rules: Rules,
    ) -> TypeRef {
        let result = match typ {
            TypeRef::Owned(TypeId::TypeParameter(id))
            | TypeRef::Uni(TypeId::TypeParameter(id))
            | TypeRef::Infer(TypeId::TypeParameter(id)) => {
                self.resolve_type_parameter(typ, id, arguments, rules)
            }
            TypeRef::Ref(TypeId::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, rules)
                .as_ref(self.db),
            TypeRef::Mut(TypeId::TypeParameter(id)) => self
                .resolve_type_parameter(typ, id, arguments, rules)
                .as_mut(self.db),
            TypeRef::Placeholder(id) => id
                .value(self.db)
                .map_or(typ, |v| self.resolve(v, arguments, rules)),
            _ => typ,
        };

        match result {
            TypeRef::Infer(TypeId::TypeParameter(id))
                if rules.infer_as_rigid =>
            {
                TypeRef::Owned(TypeId::RigidTypeParameter(id))
            }
            _ => result,
        }
    }

    fn resolve_type_parameter(
        &self,
        typ: TypeRef,
        id: TypeParameterId,
        arguments: &TypeArguments,
        rules: Rules,
    ) -> TypeRef {
        match arguments.get(id) {
            Some(arg @ TypeRef::Placeholder(id)) => id
                .value(self.db)
                .map(|v| self.resolve(v, arguments, rules))
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
        closure, generic_instance_id, generic_trait_instance,
        generic_trait_instance_id, immutable, implement, infer, instance,
        mutable, new_class, new_extern_class, new_parameter, new_trait, owned,
        parameter, placeholder, rigid, trait_instance, trait_instance_id,
        type_arguments, type_bounds, uni,
    };
    use crate::{
        Block, ClassId, Closure, TraitImplementation, TypePlaceholder,
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
    fn check_err(db: &Database, left: TypeRef, right: TypeRef) {
        assert!(
            !TypeChecker::check(db, left, right),
            "Expected {} to not be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    #[test]
    fn test_any() {
        let mut db = Database::new();
        let to_string = new_trait(&mut db, "ToString");
        let param1 = new_parameter(&mut db, "T");
        let param2 = new_parameter(&mut db, "T");
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let var2 = TypePlaceholder::alloc(&mut db, Some(param1));
        let var3 = TypePlaceholder::alloc(&mut db, Some(param2));

        param2.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_ok(&db, TypeRef::Any, TypeRef::Any);
        check_ok(&db, TypeRef::Any, TypeRef::Error);
        check_ok(&db, TypeRef::Any, placeholder(var1));
        check_ok(&db, TypeRef::Any, placeholder(var2));

        check_err(&db, TypeRef::Any, placeholder(var3));
        check_err(&db, TypeRef::Any, TypeRef::Never);
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
        check_ok(&db, TypeRef::Never, TypeRef::Any);
        check_ok(&db, TypeRef::Never, TypeRef::Never);
    }

    #[test]
    fn test_owned_class_instance() {
        let mut db = Database::new();
        let foo = new_class(&mut db, "Foo");
        let bar = new_class(&mut db, "Bar");
        let int = ClassId::int();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let to_string = new_trait(&mut db, "ToString");
        let param = new_parameter(&mut db, "T");
        let var2 = TypePlaceholder::alloc(&mut db, Some(param));
        let var3 = TypePlaceholder::alloc(&mut db, Some(param));

        param.add_requirements(&mut db, vec![trait_instance(to_string)]);
        implement(&mut db, trait_instance(to_string), bar);

        check_ok(&db, owned(instance(foo)), owned(instance(foo)));
        check_ok(&db, owned(instance(foo)), infer(instance(foo)));

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
        check_ok(&db, owned(instance(int)), mutable(instance(int)));
        check_ok(&db, owned(instance(int)), uni(instance(int)));

        check_ok(&db, owned(instance(foo)), TypeRef::Any);
        check_ok(&db, owned(instance(foo)), TypeRef::Error);

        check_err(&db, owned(instance(foo)), immutable(instance(foo)));
        check_err(&db, owned(instance(foo)), mutable(instance(foo)));
        check_err(&db, owned(instance(foo)), owned(instance(bar)));
        check_err(&db, owned(instance(foo)), TypeRef::Never);
    }

    #[test]
    fn test_extern_class_instance() {
        let mut db = Database::new();
        let foo = new_extern_class(&mut db, "Foo");
        let bar = new_extern_class(&mut db, "Bar");
        let param = new_parameter(&mut db, "T");

        check_ok(&db, owned(instance(foo)), owned(instance(foo)));

        check_err(&db, owned(instance(foo)), owned(instance(bar)));
        check_err(&db, owned(instance(foo)), TypeRef::Any);
        check_err(&db, owned(instance(foo)), owned(parameter(param)));
        check_err(&db, uni(instance(foo)), owned(parameter(param)));
    }

    #[test]
    fn test_owned_generic_class_instance() {
        let mut db = Database::new();
        let array = new_class(&mut db, "Array");
        let thing = new_class(&mut db, "Thing");
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
                vec![infer(parameter(v_param))],
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
                vec![infer(parameter(bound))],
            );

            bound.add_requirements(&mut db, vec![bound_eq]);

            let array_t = owned(generic_instance_id(
                &mut db,
                array,
                vec![infer(parameter(bound))],
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
            generic_instance_id(&mut db, array, vec![owned(instance(thing))]);
        let things2 =
            generic_instance_id(&mut db, array, vec![owned(instance(thing))]);
        let thing_refs = generic_instance_id(
            &mut db,
            array,
            vec![immutable(instance(thing))],
        );
        let floats =
            generic_instance_id(&mut db, array, vec![TypeRef::float()]);
        let vars = generic_instance_id(&mut db, array, vec![placeholder(var)]);
        let eq_things =
            generic_trait_instance_id(&mut db, equal, vec![owned(things1)]);
        let things_empty = generic_instance_id(&mut db, array, Vec::new());

        check_ok(&db, owned(things1), owned(things1));
        check_ok(&db, owned(things1), owned(things2));
        check_ok(&db, owned(things1), owned(trait_instance_id(length)));
        check_ok(&db, owned(floats), owned(trait_instance_id(length)));
        check_ok(&db, owned(things1), owned(trait_instance_id(to_string)));

        check_ok(&db, owned(vars), owned(trait_instance_id(to_string)));
        assert!(var.value(&db).is_none());

        check_ok(&db, owned(things1), owned(eq_things));
        check_ok(&db, owned(things1), infer(parameter(v_param)));
        check_ok(&db, owned(thing_refs), owned(parameter(v_param)));

        check_err(&db, owned(things1), owned(things_empty));
        check_err(&db, owned(things1), owned(floats));
        check_err(&db, owned(floats), owned(trait_instance_id(to_string)));
        check_err(&db, owned(floats), infer(parameter(v_param)));
    }

    #[test]
    fn test_uni_class_instance() {
        let mut db = Database::new();
        let foo = new_class(&mut db, "Foo");
        let bar = new_class(&mut db, "Bar");
        let int = ClassId::int();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let to_string = new_trait(&mut db, "ToString");
        let param = new_parameter(&mut db, "T");
        let var2 = TypePlaceholder::alloc(&mut db, Some(param));
        let var3 = TypePlaceholder::alloc(&mut db, Some(param));

        param.add_requirements(&mut db, vec![trait_instance(to_string)]);
        implement(&mut db, trait_instance(to_string), bar);

        check_ok(&db, uni(instance(foo)), uni(instance(foo)));
        check_ok(&db, uni(instance(foo)), owned(instance(foo)));
        check_ok(&db, uni(instance(foo)), infer(instance(foo)));

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

        check_ok(&db, uni(instance(foo)), TypeRef::Any);
        check_ok(&db, uni(instance(foo)), TypeRef::Error);

        check_err(&db, uni(instance(foo)), immutable(instance(foo)));
        check_err(&db, uni(instance(foo)), mutable(instance(foo)));
        check_err(&db, uni(instance(foo)), uni(instance(bar)));
        check_err(&db, uni(instance(foo)), TypeRef::Never);
    }

    #[test]
    fn test_uni_generic_class_instance() {
        let mut db = Database::new();
        let array = new_class(&mut db, "Array");
        let thing = new_class(&mut db, "Thing");
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
                vec![infer(parameter(v_param))],
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
                vec![infer(parameter(bound))],
            );

            bound.add_requirements(&mut db, vec![bound_eq]);

            let array_t = uni(generic_instance_id(
                &mut db,
                array,
                vec![infer(parameter(bound))],
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
            generic_instance_id(&mut db, array, vec![uni(instance(thing))]);
        let things2 =
            generic_instance_id(&mut db, array, vec![uni(instance(thing))]);
        let thing_refs = generic_instance_id(
            &mut db,
            array,
            vec![immutable(instance(thing))],
        );
        let floats =
            generic_instance_id(&mut db, array, vec![TypeRef::float()]);
        let vars = generic_instance_id(&mut db, array, vec![placeholder(var)]);
        let eq_things =
            generic_trait_instance_id(&mut db, equal, vec![uni(things1)]);

        check_ok(&db, uni(things1), uni(things1));
        check_ok(&db, uni(things1), uni(things2));
        check_ok(&db, uni(things1), uni(trait_instance_id(length)));
        check_ok(&db, uni(floats), uni(trait_instance_id(length)));
        check_ok(&db, uni(things1), uni(trait_instance_id(to_string)));

        check_ok(&db, uni(vars), uni(trait_instance_id(to_string)));
        assert!(var.value(&db).is_none());

        check_ok(&db, uni(things1), uni(eq_things));
        check_ok(&db, uni(things1), infer(parameter(v_param)));
        check_ok(&db, uni(things1), uni(parameter(v_param)));

        check_err(&db, uni(thing_refs), uni(parameter(v_param)));
        check_err(&db, uni(things1), uni(floats));
        check_err(&db, uni(floats), uni(trait_instance_id(to_string)));
        check_err(&db, uni(floats), infer(parameter(v_param)));
    }

    #[test]
    fn test_infer() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, infer(parameter(param1)), infer(parameter(param2)));
        check_ok(&db, infer(parameter(param1)), immutable(parameter(param2)));
        check_ok(&db, infer(parameter(param1)), TypeRef::Any);
        check_ok(&db, infer(parameter(param1)), TypeRef::Error);

        check_ok(&db, infer(parameter(param1)), placeholder(var));
        assert_eq!(var.value(&db), Some(infer(parameter(param1))));

        check_err(&db, infer(parameter(param1)), owned(parameter(param2)));
        check_err(&db, infer(parameter(param1)), uni(parameter(param2)));
        check_err(&db, infer(parameter(param1)), mutable(parameter(param2)));
    }

    #[test]
    fn test_ref() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let int = ClassId::int();
        let var = TypePlaceholder::alloc(&mut db, None);
        let param = new_parameter(&mut db, "T");
        let mutable_var = TypePlaceholder::alloc(&mut db, Some(param));

        param.set_mutable(&mut db);

        check_ok(&db, immutable(instance(thing)), immutable(instance(thing)));
        check_ok(&db, immutable(instance(thing)), infer(instance(thing)));

        // Value types can be passed around this way.
        check_ok(&db, immutable(instance(int)), mutable(instance(int)));
        check_ok(&db, immutable(instance(int)), owned(instance(int)));
        check_ok(&db, immutable(instance(int)), uni(instance(int)));

        check_ok(&db, immutable(instance(thing)), placeholder(var));
        assert_eq!(var.value(&db), Some(immutable(instance(thing))));

        check_ok(&db, immutable(instance(thing)), TypeRef::Any);
        check_ok(&db, immutable(instance(thing)), TypeRef::Error);
        check_ok(&db, immutable(instance(int)), infer(parameter(param)));
        check_ok(&db, immutable(instance(int)), placeholder(mutable_var));

        check_err(&db, immutable(instance(thing)), mutable(instance(thing)));
        check_err(&db, immutable(instance(thing)), owned(instance(thing)));
        check_err(&db, immutable(instance(thing)), infer(parameter(param)));
        check_err(&db, immutable(instance(thing)), placeholder(mutable_var));
    }

    #[test]
    fn test_mut() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let int = ClassId::int();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, mutable(instance(thing)), immutable(instance(thing)));
        check_ok(&db, mutable(instance(thing)), mutable(instance(thing)));
        check_ok(&db, mutable(instance(thing)), infer(instance(thing)));

        // Value types can be passed around this way.
        check_ok(&db, mutable(instance(int)), owned(instance(int)));
        check_ok(&db, mutable(instance(int)), uni(instance(int)));

        check_ok(&db, mutable(instance(thing)), placeholder(var));
        assert_eq!(var.value(&db), Some(mutable(instance(thing))));

        check_ok(&db, mutable(instance(thing)), TypeRef::Any);
        check_ok(&db, mutable(instance(thing)), TypeRef::Error);

        check_err(&db, mutable(instance(thing)), owned(instance(thing)));
        check_err(&db, mutable(instance(thing)), uni(instance(thing)));
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
        assert_eq!(var.value(&db), Some(owned(rigid(param))));
    }

    #[test]
    fn test_ref_uni() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(
            &db,
            TypeRef::RefUni(instance(thing)),
            TypeRef::RefUni(instance(thing)),
        );
        check_ok(&db, TypeRef::RefUni(instance(thing)), TypeRef::Error);

        check_err(
            &db,
            TypeRef::RefUni(instance(thing)),
            TypeRef::MutUni(instance(thing)),
        );
        check_err(&db, TypeRef::RefUni(instance(thing)), placeholder(var));
        check_err(&db, TypeRef::RefUni(instance(thing)), TypeRef::Any);
    }

    #[test]
    fn test_mut_uni() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(
            &db,
            TypeRef::MutUni(instance(thing)),
            TypeRef::MutUni(instance(thing)),
        );
        check_ok(&db, TypeRef::MutUni(instance(thing)), TypeRef::Error);

        check_err(
            &db,
            TypeRef::MutUni(instance(thing)),
            TypeRef::RefUni(instance(thing)),
        );
        check_err(&db, TypeRef::MutUni(instance(thing)), placeholder(var));
        check_err(&db, TypeRef::MutUni(instance(thing)), TypeRef::Any);
    }

    #[test]
    fn test_placeholder() {
        let mut db = Database::new();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, placeholder(var), TypeRef::int());
        assert_eq!(var.value(&db), Some(TypeRef::int()));
    }

    #[test]
    fn test_class_with_trait() {
        let mut db = Database::new();
        let animal = new_trait(&mut db, "Animal");
        let cat = new_class(&mut db, "Cat");
        let array = ClassId::array();

        array.new_type_parameter(&mut db, "T".to_string());
        implement(&mut db, trait_instance(animal), cat);

        // Array[Cat]
        let cats = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(instance(cat))],
        ));

        // ref Array[Cat]
        let ref_cats = immutable(generic_instance_id(
            &mut db,
            array,
            vec![owned(instance(cat))],
        ));

        // mut Array[Cat]
        let mut_cats = mutable(generic_instance_id(
            &mut db,
            array,
            vec![owned(instance(cat))],
        ));

        // Array[Animal]
        let animals = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        // ref Array[Animal]
        let ref_animals = immutable(generic_instance_id(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        // mut Array[Animal]
        let mut_animals = mutable(generic_instance_id(
            &mut db,
            array,
            vec![owned(trait_instance_id(animal))],
        ));

        check_ok(&db, cats, animals);
        check_ok(&db, ref_cats, ref_animals);
        check_ok(&db, mut_cats, ref_animals);

        // This isn't OK as this could result in a Dog being added to the Array.
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
        let thing = new_class(&mut db, "Thing");

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

        check_ok(
            &db,
            owned(parameter(param1)),
            owned(trait_instance_id(equal)),
        );
        check_ok(
            &db,
            owned(parameter(param3)),
            owned(trait_instance_id(equal)),
        );
        check_ok(
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
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let param3 = new_parameter(&mut db, "C");
        let param4 = new_parameter(&mut db, "D");
        let equal = new_trait(&mut db, "Equal");
        let test = new_trait(&mut db, "Test");

        test.add_required_trait(&mut db, trait_instance(equal));
        param3.add_requirements(&mut db, vec![trait_instance(equal)]);
        param4.add_requirements(&mut db, vec![trait_instance(test)]);

        check_ok(&db, owned(parameter(param1)), owned(parameter(param2)));
        check_ok(&db, owned(parameter(param4)), owned(parameter(param3)));
        check_err(&db, owned(parameter(param3)), owned(parameter(param4)));
    }

    #[test]
    fn test_type_parameter_ref_assigned_to_owned() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "A");
        let thing = new_class(&mut db, "Thing");
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
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, owned(rigid(param1)), TypeRef::Any);
        check_ok(&db, owned(rigid(param1)), TypeRef::Error);
        check_ok(&db, immutable(rigid(param1)), immutable(rigid(param1)));
        check_ok(&db, owned(rigid(param1)), owned(rigid(param1)));
        check_ok(&db, owned(rigid(param1)), infer(rigid(param1)));
        check_ok(&db, owned(rigid(param1)), infer(parameter(param1)));
        check_ok(&db, immutable(rigid(param1)), immutable(parameter(param1)));

        check_ok(&db, owned(rigid(param1)), placeholder(var));
        assert_eq!(var.value(&db), Some(owned(rigid(param1))));

        check_err(&db, owned(rigid(param1)), owned(rigid(param2)));
        check_err(&db, immutable(rigid(param1)), immutable(rigid(param2)));
        check_err(&db, owned(rigid(param1)), owned(parameter(param1)));
    }

    #[test]
    fn test_rigid_with_trait() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let to_string = new_trait(&mut db, "ToString");
        let equal = new_trait(&mut db, "Equal");

        param1.add_requirements(&mut db, vec![trait_instance(to_string)]);

        check_ok(
            &db,
            immutable(rigid(param1)),
            immutable(trait_instance_id(to_string)),
        );
        check_ok(&db, owned(rigid(param1)), infer(parameter(param2)));

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

        fun1.set_return_type(&mut db, TypeRef::Any);
        fun2.set_return_type(&mut db, TypeRef::Any);

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

        fun1.new_argument(&mut db, "a".to_string(), int, int);
        fun2.new_argument(&mut db, "b".to_string(), int, int);
        fun4.new_argument(&mut db, "a".to_string(), float, float);
        fun1.set_return_type(&mut db, TypeRef::Any);
        fun2.set_return_type(&mut db, TypeRef::Any);
        fun3.set_return_type(&mut db, TypeRef::Any);
        fun4.set_return_type(&mut db, TypeRef::Any);

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
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");

        param2.add_requirements(&mut db, vec![trait_instance(equal)]);

        check_ok(&db, owned(closure(fun)), owned(parameter(param1)));
        check_err(&db, owned(closure(fun)), owned(parameter(param2)));
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
        let array = ClassId::array();
        let var = TypePlaceholder::alloc(&mut db, None);

        array.new_type_parameter(&mut db, "T".to_string());

        let given =
            owned(generic_instance_id(&mut db, array, vec![placeholder(var)]));
        let ints =
            owned(generic_instance_id(&mut db, array, vec![TypeRef::int()]));
        let exp = owned(generic_instance_id(&mut db, array, vec![ints]));

        var.assign(&db, given);
        check_err(&db, given, exp);
    }

    #[test]
    fn test_mutable_bounds() {
        let mut db = Database::new();
        let array = ClassId::array();
        let thing = new_class(&mut db, "Thing");
        let update = new_trait(&mut db, "Update");
        let param = array.new_type_parameter(&mut db, "T".to_string());
        let bound = new_parameter(&mut db, "T");

        bound.set_mutable(&mut db);
        array.add_trait_implementation(
            &mut db,
            TraitImplementation {
                instance: trait_instance(update),
                bounds: type_bounds(vec![(param, bound)]),
            },
        );

        // Array[Thing]
        let owned_things = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(instance(thing))],
        ));

        // Array[ref Thing]
        let ref_things = owned(generic_instance_id(
            &mut db,
            array,
            vec![immutable(instance(thing))],
        ));

        check_ok(&db, owned_things, owned(trait_instance_id(update)));

        // `ref Thing` isn't mutable, so this check should fail.
        check_err(&db, ref_things, owned(trait_instance_id(update)));
    }

    #[test]
    fn test_array_of_generic_classes_with_traits() {
        let mut db = Database::new();
        let iter = new_trait(&mut db, "Iter");
        let array = ClassId::array();

        array.new_type_parameter(&mut db, "ArrayT".to_string());
        iter.new_type_parameter(&mut db, "IterT".to_string());

        let iterator = new_class(&mut db, "Iterator");
        let iterator_param =
            iterator.new_type_parameter(&mut db, "IteratorT".to_string());

        // impl Iter[T] for Iterator
        let iter_impl = TraitImplementation {
            instance: generic_trait_instance(
                &mut db,
                iter,
                vec![infer(parameter(iterator_param))],
            ),
            bounds: TypeBounds::new(),
        };

        iterator.add_trait_implementation(&mut db, iter_impl);

        let int_iterator =
            owned(generic_instance_id(&mut db, iterator, vec![TypeRef::int()]));

        let int_iter = owned(generic_trait_instance_id(
            &mut db,
            iter,
            vec![TypeRef::int()],
        ));

        // Array[Iterator[Int]]
        let lhs =
            owned(generic_instance_id(&mut db, array, vec![int_iterator]));

        // Array[Iter[Int]]
        let rhs = owned(generic_instance_id(&mut db, array, vec![int_iter]));

        check_ok(&db, lhs, rhs);
    }

    #[test]
    fn test_rigid_type_parameter() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let param = new_parameter(&mut db, "T");
        let args = type_arguments(vec![(param, owned(instance(thing)))]);
        let mut env = Environment::new(args.clone(), args);
        let res = TypeChecker::new(&db).run(
            owned(instance(thing)),
            owned(rigid(param)),
            &mut env,
        );

        assert!(!res);
        check_ok(&db, owned(rigid(param)), infer(parameter(param)));
    }

    #[test]
    fn test_rigid_type_parameter_with_requirements_with_placeholder() {
        let mut db = Database::new();
        let equal = new_trait(&mut db, "Equal");
        let param1 = new_parameter(&mut db, "T");
        let param2 = new_parameter(&mut db, "T");
        let var = TypePlaceholder::alloc(&mut db, Some(param2));

        equal.new_type_parameter(&mut db, "T".to_string());

        let param1_req = generic_trait_instance(
            &mut db,
            equal,
            vec![infer(parameter(param1))],
        );

        let param2_req = generic_trait_instance(
            &mut db,
            equal,
            vec![infer(parameter(param2))],
        );

        param1.add_requirements(&mut db, vec![param1_req]);
        param2.add_requirements(&mut db, vec![param2_req]);

        let args = type_arguments(vec![(param2, placeholder(var))]);
        let mut env = Environment::new(TypeArguments::new(), args);
        let res = TypeChecker::new(&db).run(
            owned(rigid(param1)),
            infer(parameter(param2)),
            &mut env,
        );

        assert!(res);
    }

    #[test]
    fn test_check_argument_with_mut() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
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

        assert!(TypeChecker::new(&db).check_argument(
            owned(instance(thing)),
            mutable(instance(thing)),
            mutable(trait_instance_id(to_string)),
            &mut env,
        ));
    }

    #[test]
    fn test_check_argument_with_ref() {
        let mut db = Database::new();
        let array = ClassId::array();
        let int = ClassId::int();
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

        let to_string_array = generic_instance_id(
            &mut db,
            array,
            vec![owned(trait_instance_id(to_string))],
        );

        let int_array =
            generic_instance_id(&mut db, array, vec![owned(instance(int))]);

        assert!(TypeChecker::new(&db).check_argument(
            owned(to_string_array),
            immutable(int_array),
            immutable(to_string_array),
            &mut env,
        ));
    }
}
