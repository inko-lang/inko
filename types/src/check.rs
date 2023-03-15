//! Type checking of types.
use crate::{
    Arguments, ClassInstance, Database, MethodId, TraitInstance, TypeArguments,
    TypeBounds, TypeId, TypeParameterId, TypePlaceholderId, TypeRef,
};
use std::collections::HashSet;

/// Data for a particular type-checking scope.
///
/// The type-checking process may need access to data/configuration that changes
/// based on what part of a type is checked. For example, an inner type may need
/// different type arguments compared to an outer type.
pub struct TypeCheckScope {
    /// The type arguments to expose to types on the left-hand side of the
    /// check.
    left_arguments: TypeArguments,

    /// The type arguments to expose to types on the right-hand side of the
    /// check.
    right_arguments: TypeArguments,

    /// When set to `true`, subtyping of types through traits is allowed.
    subtyping: bool,

    /// When set to `true`, owned types can be type checked against reference
    /// types.
    relaxed_ownership: bool,
}

impl TypeCheckScope {
    pub fn new(
        left_arguments: TypeArguments,
        right_arguments: TypeArguments,
    ) -> TypeCheckScope {
        TypeCheckScope {
            left_arguments,
            right_arguments,
            subtyping: true,
            relaxed_ownership: false,
        }
    }

    fn with_left_as_right(&self) -> TypeCheckScope {
        TypeCheckScope {
            left_arguments: self.left_arguments.clone(),
            right_arguments: self.left_arguments.clone(),
            subtyping: self.subtyping,
            relaxed_ownership: self.relaxed_ownership,
        }
    }

    fn without_subtyping(&self) -> TypeCheckScope {
        TypeCheckScope {
            left_arguments: self.left_arguments.clone(),
            right_arguments: self.right_arguments.clone(),
            subtyping: false,
            relaxed_ownership: self.relaxed_ownership,
        }
    }
}

/// A type for checking if two types are compatible with each other.
pub struct TypeChecker<'a> {
    db: &'a Database,
    checked: HashSet<(TypeRef, TypeRef)>,
}

impl<'a> TypeChecker<'a> {
    pub fn check(
        db: &'a Database,
        left: TypeRef,
        right: TypeRef,
    ) -> Result<(), ()> {
        let largs = if left.is_generic(db) {
            left.type_arguments(db)
        } else {
            TypeArguments::new()
        };

        let mut scope = TypeCheckScope::new(largs, TypeArguments::new());

        TypeChecker::new(db).run(left, right, &mut scope)
    }

    pub fn new(db: &'a Database) -> TypeChecker<'a> {
        TypeChecker { db, checked: HashSet::new() }
    }

    pub fn run(
        mut self,
        left: TypeRef,
        right: TypeRef,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        self.check_type_ref(left, right, scope)
    }

    pub fn check_method(
        mut self,
        left: MethodId,
        right: MethodId,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        let lhs = left.get(self.db);
        let rhs = right.get(self.db);

        if lhs.kind != rhs.kind {
            return Err(());
        }

        if lhs.visibility != rhs.visibility {
            return Err(());
        }

        if lhs.name != rhs.name {
            return Err(());
        }

        if lhs.type_parameters.len() != rhs.type_parameters.len() {
            return Err(());
        }

        let mut params_scope = scope.without_subtyping();

        lhs.type_parameters
            .values()
            .iter()
            .zip(rhs.type_parameters.values().iter())
            .try_for_each(|(&lhs, &rhs)| {
                self.check_parameters(lhs, rhs, &mut params_scope)
            })?;

        self.check_arguments(&lhs.arguments, &rhs.arguments, scope, true)?;
        self.check_type_ref(lhs.throw_type, rhs.throw_type, scope)?;
        self.check_type_ref(lhs.return_type, rhs.return_type, scope)
    }

    pub fn check_bounds(
        &mut self,
        bounds: &TypeBounds,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
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
        for (&param, &bound) in bounds.iter() {
            let val = scope.left_arguments.get(param).unwrap();

            scope.left_arguments.assign(bound, val);

            let mut new_scope = scope.with_left_as_right();

            new_scope.relaxed_ownership = true;

            if bound.is_mutable(self.db) && !val.is_mutable(self.db) {
                return Err(());
            }

            bound.requirements(self.db).into_iter().try_for_each(|r| {
                self.check_type_ref_with_trait(val, r, &mut new_scope)
            })?;
        }

        Ok(())
    }

    fn check_type_ref(
        &mut self,
        left: TypeRef,
        right: TypeRef,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        if !self.checked.insert((left, right)) {
            return Ok(());
        }

        // Resolve any assigned type parameters/placeholders to the types
        // they're assigned to.
        let left = self.resolve(left, &scope.left_arguments);
        let right = self.resolve(right, &scope.right_arguments);

        // If at this point we encounter a type placeholder, it means the
        // placeholder is yet to be assigned a value.
        match left {
            TypeRef::Any => match right {
                TypeRef::Any | TypeRef::Error => Ok(()),
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    id.required(self.db)
                        .map_or(true, |p| p.requirements(self.db).is_empty())
                        .then(|| ())
                        .ok_or(())
                }
                _ => Err(()),
            },
            // A `Never` can't be passed around because it, well, would never
            // happen. We allow the comparison so code such as `try else panic`
            // (where `panic` returns `Never`) is valid.
            TypeRef::Never => match right {
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    Ok(())
                }
                _ => Ok(()),
            },
            // Type errors are compatible with all other types to prevent a
            // cascade of type errors.
            TypeRef::Error => match right {
                TypeRef::Placeholder(id) => {
                    id.assign(self.db, left);
                    Ok(())
                }
                _ => Ok(()),
            },
            // Rigid values are more restrictive when it comes to ownership, as
            // at compile-time we can't always know the exact ownership (i.e.
            // the parameter may be a ref at runtime).
            TypeRef::Owned(left_id @ TypeId::RigidTypeParameter(lhs)) => {
                match right {
                    TypeRef::Infer(right_id) => {
                        self.check_rigid_with_type_id(lhs, right_id, scope)
                    }
                    TypeRef::Placeholder(id) => self
                        .check_type_id_with_placeholder(
                            left, left_id, id, scope,
                        ),
                    TypeRef::Any | TypeRef::Error => Ok(()),
                    _ => Err(()),
                }
            }
            TypeRef::Owned(left_id) => match right {
                TypeRef::Owned(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Ref(right_id)
                | TypeRef::Mut(right_id)
                | TypeRef::Uni(right_id)
                    if left.is_value_type(self.db)
                        || scope.relaxed_ownership =>
                {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(left, left_id, id, scope),
                TypeRef::Any | TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::Uni(left_id) => match right {
                TypeRef::Owned(right_id)
                | TypeRef::Infer(right_id)
                | TypeRef::Uni(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Ref(right_id) | TypeRef::Mut(right_id)
                    if left.is_value_type(self.db)
                        || scope.relaxed_ownership =>
                {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(left, left_id, id, scope),
                TypeRef::Any | TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::Infer(left_id) => match right {
                // Mut and Owned are not allowed because we don't know the
                // runtime ownership of our value. Ref is fine, because we can
                // always turn an Owned/Ref/Mut/etc into a Ref.
                TypeRef::Infer(right_id) | TypeRef::Ref(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(left, left_id, id, scope),
                TypeRef::Any | TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::Ref(left_id) => match right {
                TypeRef::Ref(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Owned(right_id)
                | TypeRef::Uni(right_id)
                | TypeRef::Mut(right_id)
                    if left.is_value_type(self.db)
                        || scope.relaxed_ownership =>
                {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(left, left_id, id, scope),
                TypeRef::Any | TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::Mut(left_id) => match right {
                TypeRef::Ref(right_id) | TypeRef::Infer(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Mut(right_id) => self.check_type_id(
                    left_id,
                    right_id,
                    &mut scope.without_subtyping(),
                ),
                TypeRef::Owned(right_id) | TypeRef::Uni(right_id)
                    if left.is_value_type(self.db)
                        || scope.relaxed_ownership =>
                {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Placeholder(id) => self
                    .check_type_id_with_placeholder(left, left_id, id, scope),
                TypeRef::Any | TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::RefUni(left_id) => match right {
                TypeRef::RefUni(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::MutUni(left_id) => match right {
                TypeRef::MutUni(right_id) => {
                    self.check_type_id(left_id, right_id, scope)
                }
                TypeRef::Error => Ok(()),
                _ => Err(()),
            },
            TypeRef::Placeholder(left_id) => {
                // If we reach this point it means the placeholder isn't
                // assigned a value.
                left_id.assign(self.db, right);
                Ok(())
            }
            _ => Err(()),
        }
    }

    fn check_type_id(
        &mut self,
        left_id: TypeId,
        right_id: TypeId,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        match left_id {
            TypeId::Class(_) | TypeId::Trait(_) | TypeId::Module(_) => {
                // Classes, traits and modules themselves aren't treated as
                // types and thus can't be passed around, mostly because this
                // just isn't useful. To further reinforce this, these types
                // aren't compatible with anything.
                Err(())
            }
            TypeId::ClassInstance(lhs) => match right_id {
                TypeId::ClassInstance(rhs) => {
                    if lhs.instance_of != rhs.instance_of {
                        return Err(());
                    }

                    if !lhs.instance_of.is_generic(self.db) {
                        return Ok(());
                    }

                    let lhs_args = lhs.type_arguments(self.db);
                    let rhs_args = rhs.type_arguments(self.db);

                    lhs.instance_of
                        .type_parameters(self.db)
                        .into_iter()
                        .try_for_each(|param| {
                            let lhs = lhs_args.get(param).unwrap();
                            let rhs = rhs_args.get(param).unwrap();

                            self.check_type_ref(lhs, rhs, scope)
                        })
                }
                TypeId::TraitInstance(rhs) => {
                    self.check_class_with_trait(lhs, rhs, scope)
                }
                TypeId::TypeParameter(rhs) => {
                    rhs.requirements(self.db).into_iter().try_for_each(|req| {
                        self.check_class_with_trait(lhs, req, scope)
                    })
                }
                _ => Err(()),
            },
            TypeId::TraitInstance(lhs) => match right_id {
                TypeId::TraitInstance(rhs) => {
                    self.check_traits(lhs, rhs, scope)
                }
                TypeId::TypeParameter(rhs) => rhs
                    .requirements(self.db)
                    .into_iter()
                    .try_for_each(|req| self.check_traits(lhs, req, scope)),
                _ => Err(()),
            },
            TypeId::TypeParameter(lhs) => match right_id {
                TypeId::TraitInstance(rhs) => {
                    self.check_parameter_with_trait(lhs, rhs, scope)
                }
                TypeId::TypeParameter(rhs) => {
                    self.check_parameters(lhs, rhs, scope)
                }
                _ => Err(()),
            },
            TypeId::RigidTypeParameter(lhs) => {
                self.check_rigid_with_type_id(lhs, right_id, scope)
            }
            TypeId::Closure(lhs) => match right_id {
                TypeId::Closure(rhs) => {
                    let lhs_obj = lhs.get(self.db);
                    let rhs_obj = rhs.get(self.db);

                    self.check_arguments(
                        &lhs_obj.arguments,
                        &rhs_obj.arguments,
                        scope,
                        false,
                    )?;
                    self.check_type_ref(
                        lhs_obj.throw_type,
                        rhs_obj.throw_type,
                        scope,
                    )?;
                    self.check_type_ref(
                        lhs_obj.return_type,
                        rhs_obj.return_type,
                        scope,
                    )
                }
                TypeId::TypeParameter(rhs)
                    if rhs.requirements(self.db).is_empty() =>
                {
                    // Closures can't implement traits, so they're only
                    // compatible with type parameters that don't have any
                    // requirements.
                    Ok(())
                }
                _ => Err(()),
            },
        }
    }

    fn check_rigid_with_type_id(
        &mut self,
        left: TypeParameterId,
        right: TypeId,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        match right {
            TypeId::RigidTypeParameter(rhs) if left == rhs => Ok(()),
            TypeId::TraitInstance(rhs) => {
                self.check_parameter_with_trait(left, rhs, scope)
            }
            TypeId::TypeParameter(rhs) => {
                if left == rhs {
                    return Ok(());
                }

                // LHS must meet _all_ of the requirements of RHS.
                let lhs_reqs = left.requirements(self.db);

                rhs.requirements(self.db)
                    .into_iter()
                    .all(|rreq| {
                        lhs_reqs.iter().any(|lreq| {
                            self.check_traits(*lreq, rreq, scope).is_ok()
                        })
                    })
                    .then(|| ())
                    .ok_or(())
            }
            _ => Err(()),
        }
    }

    fn check_type_id_with_placeholder(
        &mut self,
        left: TypeRef,
        left_id: TypeId,
        placeholder: TypePlaceholderId,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        // By assigning the placeholder first, recursive checks against the same
        // placeholder don't keep recursing into this method, instead checking
        // against the value on the left.
        placeholder.assign(self.db, left);

        let req = if let Some(req) = placeholder.required(self.db) {
            req
        } else {
            return Ok(());
        };

        let mut reqs = req.requirements(self.db).into_iter();

        match left_id {
            TypeId::ClassInstance(lhs) => reqs.try_for_each(|req| {
                self.check_class_with_trait(lhs, req, scope)
            }),
            TypeId::TraitInstance(lhs) => {
                reqs.try_for_each(|req| self.check_traits(lhs, req, scope))
            }
            TypeId::TypeParameter(lhs) | TypeId::RigidTypeParameter(lhs) => {
                reqs.try_for_each(|req| {
                    self.check_parameter_with_trait(lhs, req, scope)
                })
            }
            _ => Err(()),
        }
    }

    fn check_class_with_trait(
        &mut self,
        left: ClassInstance,
        right: TraitInstance,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        // `Array[Cat]` isn't compatible with `mut Array[Animal]`, as that could
        // result in a `Dog` being added to the Array.
        if !scope.subtyping {
            return Err(());
        }

        let imp = if let Some(found) =
            left.instance_of.trait_implementation(self.db, right.instance_of)
        {
            found
        } else {
            return Err(());
        };

        self.check_bounds(&imp.bounds, scope)?;
        self.check_traits(imp.instance, right, scope)
    }

    fn check_type_ref_with_trait(
        &mut self,
        left: TypeRef,
        right: TraitInstance,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        match left {
            TypeRef::Owned(id)
            | TypeRef::Uni(id)
            | TypeRef::Ref(id)
            | TypeRef::Mut(id)
            | TypeRef::RefUni(id)
            | TypeRef::MutUni(id)
            | TypeRef::Infer(id) => match id {
                TypeId::ClassInstance(lhs) => {
                    self.check_class_with_trait(lhs, right, scope)
                }
                TypeId::TraitInstance(lhs) => {
                    self.check_traits(lhs, right, scope)
                }
                TypeId::TypeParameter(lhs)
                | TypeId::RigidTypeParameter(lhs) => {
                    self.check_parameter_with_trait(lhs, right, scope)
                }
                _ => Err(()),
            },
            TypeRef::Placeholder(id) => match id.value(self.db) {
                Some(typ) => self.check_type_ref_with_trait(typ, right, scope),
                // When the placeholder isn't assigned a value, the comparison
                // is treated as valid but we don't assign a type. This is
                // because in this scenario we can't reliably guess what the
                // type is, and what its ownership should be.
                _ => Ok(()),
            },
            _ => Err(()),
        }
    }

    fn check_parameter_with_trait(
        &mut self,
        left: TypeParameterId,
        right: TraitInstance,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        left.requirements(self.db)
            .into_iter()
            .any(|req| self.check_traits(req, right, scope).is_ok())
            .then(|| ())
            .ok_or(())
    }

    fn check_parameters(
        &mut self,
        left: TypeParameterId,
        right: TypeParameterId,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        if left == right {
            return Ok(());
        }

        right.requirements(self.db).into_iter().try_for_each(|req| {
            self.check_parameter_with_trait(left, req, scope)
        })
    }

    fn check_traits(
        &mut self,
        left: TraitInstance,
        right: TraitInstance,
        scope: &mut TypeCheckScope,
    ) -> Result<(), ()> {
        if left == right {
            return Ok(());
        }

        if left.instance_of != right.instance_of {
            return if scope.subtyping {
                left.instance_of
                    .required_traits(self.db)
                    .into_iter()
                    .any(|lhs| self.check_traits(lhs, right, scope).is_ok())
                    .then(|| ())
                    .ok_or(())
            } else {
                Err(())
            };
        }

        if !left.instance_of.is_generic(self.db) {
            return Ok(());
        }

        let lhs_args = left.type_arguments(self.db);
        let rhs_args = right.type_arguments(self.db);

        left.instance_of.type_parameters(self.db).into_iter().try_for_each(
            |param| {
                let lhs = lhs_args.get(param).unwrap();
                let rhs = rhs_args.get(param).unwrap();

                self.check_type_ref(lhs, rhs, scope)
            },
        )
    }

    fn check_arguments(
        &mut self,
        left: &Arguments,
        right: &Arguments,
        scope: &mut TypeCheckScope,
        same_name: bool,
    ) -> Result<(), ()> {
        if left.len() != right.len() {
            return Err(());
        }

        let mut scope = scope.without_subtyping();

        left.mapping
            .values()
            .iter()
            .zip(right.mapping.values().iter())
            .try_for_each(|(ours, theirs)| {
                if same_name && ours.name != theirs.name {
                    return Err(());
                }

                self.check_type_ref(
                    ours.value_type,
                    theirs.value_type,
                    &mut scope,
                )
            })
    }

    fn resolve(&self, typ: TypeRef, arguments: &TypeArguments) -> TypeRef {
        match typ {
            TypeRef::Owned(TypeId::TypeParameter(id))
            | TypeRef::Uni(TypeId::TypeParameter(id))
            | TypeRef::Ref(TypeId::TypeParameter(id))
            | TypeRef::Mut(TypeId::TypeParameter(id))
            | TypeRef::Infer(TypeId::TypeParameter(id)) => {
                match arguments.get(id) {
                    Some(arg @ TypeRef::Placeholder(id)) => id
                        .value(self.db)
                        .map(|v| self.resolve(v, arguments))
                        .unwrap_or(arg),
                    Some(arg) => arg,
                    _ => typ,
                }
            }
            TypeRef::Placeholder(id) => {
                id.value(self.db).map_or(typ, |v| self.resolve(v, arguments))
            }
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
        mutable, new_class, new_parameter, new_trait, owned, parameter,
        placeholder, rigid, trait_instance, trait_instance_id, type_bounds,
        uni,
    };
    use crate::{
        Block, ClassId, Closure, TraitImplementation, TypePlaceholder,
    };

    fn check_ok(db: &Database, left: TypeRef, right: TypeRef) {
        assert!(
            TypeChecker::check(db, left, right).is_ok(),
            "Expected {} to be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    fn check_ok_relaxed(db: &Database, left: TypeRef, right: TypeRef) {
        let args = if left.is_generic(db) {
            left.type_arguments(db)
        } else {
            TypeArguments::new()
        };

        let mut scope = TypeCheckScope::new(args, TypeArguments::new());

        scope.relaxed_ownership = true;

        assert!(
            TypeChecker::new(db).run(left, right, &mut scope).is_ok(),
            "Expected {} to be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    fn check_err(db: &Database, left: TypeRef, right: TypeRef) {
        assert!(
            TypeChecker::check(db, left, right).is_err(),
            "Expected {} to not be compatible with {}",
            format_type(db, left),
            format_type(db, right)
        );
    }

    #[test]
    fn test_type_checker_any() {
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
    fn test_type_checker_never() {
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
    fn test_type_checker_owned_class_instance() {
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

        // Foo doesn't implement ToString, so the check fails. The placeholder
        // is still assigned to handle recursive types/checks.
        check_err(&db, owned(instance(foo)), placeholder(var2));
        assert_eq!(var2.value(&db), Some(owned(instance(foo))));

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
    fn test_type_checker_owned_generic_class_instance() {
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

        check_err(&db, owned(things1), owned(floats));
        check_err(&db, owned(floats), owned(trait_instance_id(to_string)));
        check_err(&db, owned(floats), infer(parameter(v_param)));
    }

    #[test]
    fn test_type_checker_uni_class_instance() {
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

        // Foo doesn't implement ToString, so the check fails. The placeholder
        // is still assigned to handle recursive types/checks.
        check_err(&db, uni(instance(foo)), placeholder(var2));
        assert_eq!(var2.value(&db), Some(uni(instance(foo))));

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
    fn test_type_checker_uni_generic_class_instance() {
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
        check_ok(&db, uni(thing_refs), uni(parameter(v_param)));

        check_err(&db, uni(things1), uni(floats));
        check_err(&db, uni(floats), uni(trait_instance_id(to_string)));
        check_err(&db, uni(floats), infer(parameter(v_param)));
    }

    #[test]
    fn test_type_checker_infer() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, infer(parameter(param1)), infer(parameter(param2)));
        check_ok(&db, infer(parameter(param1)), immutable(parameter(param2)));
        check_ok(&db, infer(parameter(param1)), TypeRef::Any);
        check_ok(&db, infer(parameter(param1)), TypeRef::Error);

        check_ok(&db, infer(parameter(param1)), placeholder(var));
        assert_eq!(var.resolve(&db), infer(parameter(param1)));

        check_err(&db, infer(parameter(param1)), owned(parameter(param2)));
        check_err(&db, infer(parameter(param1)), uni(parameter(param2)));
        check_err(&db, infer(parameter(param1)), mutable(parameter(param2)));
    }

    #[test]
    fn test_type_checker_ref() {
        let mut db = Database::new();
        let thing = new_class(&mut db, "Thing");
        let int = ClassId::int();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, immutable(instance(thing)), immutable(instance(thing)));
        check_ok(&db, immutable(instance(thing)), infer(instance(thing)));

        // Value types can be passed around this way.
        check_ok(&db, immutable(instance(int)), mutable(instance(int)));
        check_ok(&db, immutable(instance(int)), owned(instance(int)));
        check_ok(&db, immutable(instance(int)), uni(instance(int)));

        check_ok_relaxed(
            &db,
            immutable(instance(thing)),
            owned(instance(thing)),
        );
        check_ok_relaxed(&db, immutable(instance(thing)), uni(instance(thing)));

        check_ok(&db, immutable(instance(thing)), placeholder(var));
        assert_eq!(var.resolve(&db), immutable(instance(thing)));

        check_ok(&db, immutable(instance(thing)), TypeRef::Any);
        check_ok(&db, immutable(instance(thing)), TypeRef::Error);

        check_err(&db, immutable(instance(thing)), mutable(instance(thing)));
        check_err(&db, immutable(instance(thing)), owned(instance(thing)));
    }

    #[test]
    fn test_type_checker_mut() {
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

        check_ok_relaxed(&db, mutable(instance(thing)), owned(instance(thing)));
        check_ok_relaxed(&db, mutable(instance(thing)), uni(instance(thing)));

        check_ok(&db, mutable(instance(thing)), placeholder(var));
        assert_eq!(var.resolve(&db), mutable(instance(thing)));

        check_ok(&db, mutable(instance(thing)), TypeRef::Any);
        check_ok(&db, mutable(instance(thing)), TypeRef::Error);

        check_err(&db, mutable(instance(thing)), owned(instance(thing)));
        check_err(&db, mutable(instance(thing)), uni(instance(thing)));
    }

    #[test]
    fn test_type_checker_ref_uni() {
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
    fn test_type_checker_mut_uni() {
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
    fn test_type_checker_placeholder() {
        let mut db = Database::new();
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, placeholder(var), TypeRef::int());
        assert_eq!(var.resolve(&db), TypeRef::int());
    }

    #[test]
    fn test_type_checker_class_with_trait() {
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
    fn test_type_checker_traits() {
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
    fn test_type_checker_generic_traits() {
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

        check_ok(&db, eq_thing, eq_thing);
        check_ok(&db, eq_ref_thing, eq_ref_thing);
        check_ok(&db, eq_mut_thing, eq_mut_thing);
        check_err(&db, eq_thing, eq_ref_thing);
        check_err(&db, eq_thing, eq_mut_thing);
    }

    #[test]
    fn test_type_checker_type_parameter_with_trait() {
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
    fn test_type_checker_trait_with_parameter() {
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
    fn test_type_checker_parameters() {
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
    fn test_type_checker_rigid() {
        let mut db = Database::new();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let var = TypePlaceholder::alloc(&mut db, None);

        check_ok(&db, owned(rigid(param1)), TypeRef::Any);
        check_ok(&db, owned(rigid(param1)), TypeRef::Error);
        check_ok(&db, immutable(rigid(param1)), immutable(rigid(param1)));
        check_ok(&db, owned(rigid(param1)), infer(rigid(param1)));
        check_ok(&db, owned(rigid(param1)), infer(parameter(param1)));
        check_ok(&db, immutable(rigid(param1)), immutable(parameter(param1)));

        check_ok(&db, owned(rigid(param1)), placeholder(var));
        assert_eq!(var.resolve(&db), owned(rigid(param1)));

        // The rigid parameter may actually be a ref/mut at runtime, so we can't
        // allow this.
        check_err(&db, owned(rigid(param1)), owned(rigid(param1)));
        check_err(&db, owned(rigid(param1)), owned(rigid(param2)));
        check_err(&db, immutable(rigid(param1)), immutable(rigid(param2)));
        check_err(&db, owned(rigid(param1)), owned(parameter(param1)));
    }

    #[test]
    fn test_type_checker_rigid_with_trait() {
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
    fn test_type_checker_simple_closures() {
        let mut db = Database::new();
        let fun1 = Closure::alloc(&mut db, false);
        let fun2 = Closure::alloc(&mut db, false);

        fun1.set_return_type(&mut db, TypeRef::Any);
        fun2.set_return_type(&mut db, TypeRef::Any);

        check_ok(&db, owned(closure(fun1)), owned(closure(fun2)));
    }

    #[test]
    fn test_type_checker_closures_with_arguments() {
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
    fn test_type_checker_closures_with_return_types() {
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
    fn test_type_checker_closures_with_throw_types() {
        let mut db = Database::new();
        let fun1 = Closure::alloc(&mut db, false);
        let fun2 = Closure::alloc(&mut db, false);
        let fun3 = Closure::alloc(&mut db, false);
        let int = TypeRef::int();
        let float = TypeRef::float();

        fun1.set_throw_type(&mut db, int);
        fun1.set_return_type(&mut db, TypeRef::Any);
        fun2.set_throw_type(&mut db, int);
        fun2.set_return_type(&mut db, TypeRef::Any);
        fun3.set_throw_type(&mut db, float);
        fun3.set_return_type(&mut db, TypeRef::Any);

        check_ok(&db, owned(closure(fun1)), owned(closure(fun2)));
        check_err(&db, owned(closure(fun1)), owned(closure(fun3)));
    }

    #[test]
    fn test_type_checker_closure_with_parameter() {
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
    fn test_type_checker_recursive_type() {
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
    fn test_type_checker_mutable_bounds() {
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
}
