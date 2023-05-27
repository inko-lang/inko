//! Resolving abstract types into concrete types.
use crate::either::Either;
use crate::{
    ClassInstance, Closure, Database, TraitId, TraitInstance, TypeArguments,
    TypeBounds, TypeId, TypeParameterId, TypeRef,
};
use std::collections::HashMap;

/// A type that takes an abstract type and resolves it into a more concrete
/// type.
///
/// For example, if a method has any type parameter bounds then this type
/// ensures any regular type parameters are turned into their corresponding
/// bounds.
pub struct TypeResolver<'a> {
    db: &'a mut Database,

    /// A cache of types we've already resolved.
    ///
    /// This cache is used to handle recursive types, such as a type parameter
    /// assigned to a placeholder that's assigned to itself.
    cached: HashMap<TypeRef, TypeRef>,

    /// The type arguments to use when resolving type parameters.
    type_arguments: &'a TypeArguments,

    /// Any type parameters that have additional bounds set.
    ///
    /// If a type parameter is present in this structure, it's bounded version
    /// is produced when resolving the parameter.
    bounds: &'a TypeBounds,

    /// If the resolved type should be made immutable or not.
    immutable: bool,

    /// When set to true, non-rigid type parameters are turned into rigid
    /// parameters instead of placeholders.
    rigid: bool,

    /// The surrounding trait definition, if any.
    ///
    /// If present it's used to remap inherited type parameters to their correct
    /// types.
    surrounding_trait: Option<TraitId>,
}

impl<'a> TypeResolver<'a> {
    pub fn new(
        db: &'a mut Database,
        type_arguments: &'a TypeArguments,
        bounds: &'a TypeBounds,
    ) -> TypeResolver<'a> {
        TypeResolver {
            db,
            type_arguments,
            bounds,
            immutable: false,
            rigid: false,
            surrounding_trait: None,
            cached: HashMap::new(),
        }
    }

    pub fn with_immutable(mut self, immutable: bool) -> TypeResolver<'a> {
        self.immutable = immutable;
        self
    }

    pub fn with_rigid(mut self, rigid: bool) -> TypeResolver<'a> {
        self.rigid = rigid;
        self
    }

    pub fn resolve(&mut self, value: TypeRef) -> TypeRef {
        let typ = self.resolve_type_ref(value);

        if self.immutable {
            typ.as_ref(self.db)
        } else {
            typ
        }
    }

    pub fn resolve_type_ref(&mut self, value: TypeRef) -> TypeRef {
        if let Some(&cached) = self.cached.get(&value) {
            return cached;
        }

        // To handle recursive types we have to add the raw value first, then
        // later update it with the resolved value. If we don't do this we'd
        // just end up recursing into this method indefinitely. This also
        // ensures we handle type parameters assigned to themselves, without
        // needing extra logic.
        self.cached.insert(value, value);

        let resolved = match value {
            TypeRef::Owned(id) | TypeRef::Infer(id) => {
                match self.resolve_type_id(id) {
                    Either::Left(res) => TypeRef::Owned(res),
                    Either::Right(typ) => typ,
                }
            }
            TypeRef::Ref(id) => match self.resolve_type_id(id) {
                Either::Left(res) => TypeRef::Ref(res),
                Either::Right(TypeRef::Owned(typ) | TypeRef::Mut(typ)) => {
                    TypeRef::Ref(typ)
                }
                Either::Right(TypeRef::Uni(typ) | TypeRef::UniMut(typ)) => {
                    TypeRef::UniRef(typ)
                }
                Either::Right(typ) => typ,
            },
            TypeRef::Mut(id) => match self.resolve_type_id(id) {
                Either::Left(res) => TypeRef::Mut(res),
                Either::Right(TypeRef::Owned(typ)) => TypeRef::Mut(typ),
                Either::Right(TypeRef::Uni(typ)) => TypeRef::UniMut(typ),
                Either::Right(typ) => typ,
            },
            TypeRef::Uni(id) => match self.resolve_type_id(id) {
                Either::Left(res) => TypeRef::Uni(res),
                Either::Right(TypeRef::Owned(typ)) => TypeRef::Uni(typ),
                Either::Right(typ) => typ,
            },
            TypeRef::UniRef(id) => match self.resolve_type_id(id) {
                Either::Left(res) => TypeRef::UniRef(res),
                Either::Right(typ) => typ,
            },
            TypeRef::UniMut(id) => match self.resolve_type_id(id) {
                Either::Left(res) => TypeRef::UniMut(res),
                Either::Right(typ) => typ,
            },
            // If a placeholder is unassigned we need to return it as-is. This
            // way future use of the placeholder allows us to infer the current
            // type.
            TypeRef::Placeholder(id) => id
                .value(self.db)
                .map(|v| self.resolve_type_ref(v))
                .unwrap_or(value),
            _ => value,
        };

        // No point in hashing again if the value is the same.
        if value != resolved {
            self.cached.insert(value, resolved);
        }

        resolved
    }

    fn resolve_type_id(&mut self, id: TypeId) -> Either<TypeId, TypeRef> {
        match id {
            TypeId::ClassInstance(ins) => {
                let base = ins.instance_of;

                if !base.is_generic(self.db) {
                    return Either::Left(id);
                }

                let mut args = ins.type_arguments(self.db).clone();

                self.resolve_arguments(&mut args);

                Either::Left(TypeId::ClassInstance(ClassInstance::generic(
                    self.db, base, args,
                )))
            }
            TypeId::TraitInstance(ins) => {
                let base = ins.instance_of;

                if !base.is_generic(self.db) {
                    return Either::Left(id);
                }

                let mut args = ins.type_arguments(self.db).clone();

                self.resolve_arguments(&mut args);

                Either::Left(TypeId::TraitInstance(TraitInstance::generic(
                    self.db, base, args,
                )))
            }
            TypeId::TypeParameter(pid) => {
                let pid = self.remap_type_parameter(pid);

                match self.resolve_type_parameter(pid) {
                    Some(val) => Either::Right(val),
                    _ if self.rigid => {
                        Either::Left(TypeId::RigidTypeParameter(pid))
                    }
                    _ => {
                        Either::Right(TypeRef::placeholder(self.db, Some(pid)))
                    }
                }
            }
            TypeId::RigidTypeParameter(pid) => Either::Left(
                TypeId::RigidTypeParameter(self.remap_type_parameter(pid)),
            ),
            TypeId::Closure(id) => {
                let mut new = id.get(self.db).clone();
                let immutable = self.immutable;

                // The ownership of the closure's arguments and return type
                // shouldn't be changed, instead the ability to use the closure
                // in the first place is restricted by the type checker where
                // needede.
                self.immutable = false;

                for arg in new.arguments.mapping.values_mut() {
                    arg.value_type = self.resolve_type_ref(arg.value_type);
                }

                new.return_type = self.resolve_type_ref(new.return_type);
                self.immutable = immutable;
                Either::Left(TypeId::Closure(Closure::add(self.db, new)))
            }
            _ => Either::Left(id),
        }
    }

    fn resolve_arguments(&mut self, arguments: &mut TypeArguments) {
        for value in arguments.mapping.values_mut() {
            *value = self.resolve_type_ref(*value);
        }
    }

    fn resolve_type_parameter(
        &mut self,
        id: TypeParameterId,
    ) -> Option<TypeRef> {
        // Type arguments are always mapped using the original type parameters.
        // This way if we have a bounded parameter we can easily look up the
        // corresponding argument.
        let key = id.original(self.db).unwrap_or(id);

        if let Some(arg) = self.type_arguments.get(key) {
            return Some(self.resolve_type_ref(arg));
        }

        // Inside a trait we may end up referring to type parameters from a
        // required trait. In this case we recursively resolve the type
        // parameter chain until reaching the final type.
        if let Some(arg) = self
            .surrounding_trait
            .and_then(|t| t.get(self.db).inherited_type_arguments.get(key))
        {
            return Some(self.resolve_type_ref(arg));
        }

        None
    }

    fn remap_type_parameter(&self, id: TypeParameterId) -> TypeParameterId {
        self.bounds.get(id).unwrap_or(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{
        closure, generic_instance_id, generic_trait_instance,
        generic_trait_instance_id, immutable, immutable_uni, infer, instance,
        mutable, mutable_uni, new_parameter, new_trait, owned, parameter,
        placeholder, rigid, type_arguments, type_bounds, uni,
    };
    use crate::{Block, ClassId, Closure, TypePlaceholder, TypePlaceholderId};

    fn resolve(
        db: &mut Database,
        type_arguments: &TypeArguments,
        bounds: &TypeBounds,
        source: TypeRef,
    ) -> TypeRef {
        TypeResolver::new(db, type_arguments, bounds).resolve(source)
    }

    fn resolve_immutable(
        db: &mut Database,
        type_arguments: &TypeArguments,
        bounds: &TypeBounds,
        source: TypeRef,
    ) -> TypeRef {
        TypeResolver::new(db, type_arguments, bounds)
            .with_immutable(true)
            .resolve(source)
    }

    #[test]
    fn test_owned() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(instance(string))),
            owned(instance(string))
        );

        assert_eq!(
            resolve_immutable(&mut db, &args, &bounds, owned(instance(string))),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_immutable_nested_type() {
        let mut db = Database::new();
        let array = ClassId::array();
        let int = ClassId::int();

        array.new_type_parameter(&mut db, "T".to_string());

        let int_array = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(instance(int))],
        ));

        let input = owned(generic_instance_id(&mut db, array, vec![int_array]));
        let resolved = resolve_immutable(
            &mut db,
            &TypeArguments::new(),
            &TypeBounds::new(),
            input,
        );

        assert!(resolved.is_ref(&db));
        assert!(resolved.type_arguments(&db).pairs()[0].1.is_owned(&db));
    }

    #[test]
    fn test_infer() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, infer(instance(string))),
            owned(instance(string))
        );

        assert_eq!(
            resolve_immutable(&mut db, &args, &bounds, infer(instance(string))),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_uni() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, uni(instance(string))),
            uni(instance(string))
        );

        assert_eq!(
            resolve_immutable(&mut db, &args, &bounds, uni(instance(string))),
            immutable_uni(instance(string))
        );
    }

    #[test]
    fn test_ref() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable(instance(string))),
            immutable(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                immutable(instance(string))
            ),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_ref_uni() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable_uni(instance(string))),
            immutable_uni(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                immutable_uni(instance(string))
            ),
            immutable_uni(instance(string))
        );
    }

    #[test]
    fn test_mut() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable(instance(string))),
            mutable(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                mutable(instance(string))
            ),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_mut_uni() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable_uni(instance(string))),
            mutable_uni(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                mutable_uni(instance(string))
            ),
            immutable_uni(instance(string))
        );
    }

    #[test]
    fn test_placeholder() {
        let mut db = Database::new();
        let string = ClassId::string();
        let args = TypeArguments::new();
        let bounds = TypeBounds::new();
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let var2 = TypePlaceholder::alloc(&mut db, None);

        var1.assign(&db, owned(instance(string)));

        assert_eq!(
            resolve(&mut db, &args, &bounds, placeholder(var1)),
            owned(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, placeholder(var2)),
            placeholder(var2)
        );

        assert_eq!(
            resolve_immutable(&mut db, &args, &bounds, placeholder(var1)),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_type_parameter() {
        let mut db = Database::new();
        let string = ClassId::string();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let args = type_arguments(vec![(param1, owned(instance(string)))]);
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(parameter(param1))),
            owned(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(rigid(param1))),
            owned(rigid(param1))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, infer(parameter(param1))),
            owned(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                infer(parameter(param1))
            ),
            immutable(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(rigid(param2))),
            owned(rigid(param2))
        );
    }

    #[test]
    fn test_type_parameter_as_reference() {
        let mut db = Database::new();
        let string = ClassId::string();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let args = type_arguments(vec![
            (param1, owned(instance(string))),
            (param2, immutable(instance(string))),
        ]);
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable(parameter(param1))),
            immutable(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable(parameter(param2))),
            immutable(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable(parameter(param1))),
            mutable(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                mutable(parameter(param1))
            ),
            immutable(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable(parameter(param2))),
            immutable(instance(string))
        );
    }

    #[test]
    fn test_type_parameter_as_uni() {
        let mut db = Database::new();
        let string = ClassId::string();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let args = type_arguments(vec![
            (param1, uni(instance(string))),
            (param2, immutable_uni(instance(string))),
        ]);
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable(parameter(param1))),
            immutable_uni(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, immutable(parameter(param2))),
            immutable_uni(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable(parameter(param1))),
            mutable_uni(instance(string))
        );

        assert_eq!(
            resolve_immutable(
                &mut db,
                &args,
                &bounds,
                mutable(parameter(param1))
            ),
            immutable_uni(instance(string))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, mutable(parameter(param2))),
            immutable_uni(instance(string))
        );
    }

    #[test]
    fn test_type_parameter_surrounding_trait() {
        let mut db = Database::new();
        let string = ClassId::string();
        let to_foo = new_trait(&mut db, "ToFoo");
        let to_bar = new_trait(&mut db, "ToBar");
        let foo_param = to_foo.new_type_parameter(&mut db, "A".to_string());
        let bar_param = to_bar.new_type_parameter(&mut db, "B".to_string());

        {
            let ins = generic_trait_instance(
                &mut db,
                to_foo,
                vec![owned(parameter(bar_param))],
            );

            // ToBar[B]: ToFoo[B]
            to_bar.add_required_trait(&mut db, ins);
        }

        let args = type_arguments(vec![(bar_param, owned(instance(string)))]);
        let bounds = TypeBounds::new();
        let mut resolver = TypeResolver::new(&mut db, &args, &bounds);

        resolver.surrounding_trait = Some(to_bar);

        assert_eq!(
            resolver.resolve(owned(parameter(foo_param))),
            owned(instance(string))
        );
    }

    #[test]
    fn test_generic_class() {
        let mut db = Database::new();
        let array = ClassId::array();
        let string = ClassId::string();
        let param = new_parameter(&mut db, "A");
        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let args = type_arguments(vec![(param, owned(instance(string)))]);
        let bounds = TypeBounds::new();
        let input = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(parameter(param))],
        ));

        let arg = match resolve(&mut db, &args, &bounds, input) {
            TypeRef::Owned(TypeId::ClassInstance(ins)) => {
                ins.type_arguments(&db).get(array_param).unwrap()
            }
            _ => TypeRef::Unknown,
        };

        assert_eq!(arg, owned(instance(string)));
    }

    #[test]
    fn test_generic_class_with_parameter_chain() {
        let mut db = Database::new();
        let array = ClassId::array();
        let string = ClassId::string();
        let param1 = new_parameter(&mut db, "A");
        let param2 = new_parameter(&mut db, "B");
        let param3 = new_parameter(&mut db, "C");
        let array_param = array.new_type_parameter(&mut db, "T".to_string());
        let args = type_arguments(vec![
            (param1, owned(parameter(param2))),
            (param2, owned(parameter(param3))),
            (param3, owned(instance(string))),
        ]);
        let bounds = TypeBounds::new();
        let input = owned(generic_instance_id(
            &mut db,
            array,
            vec![owned(parameter(param1))],
        ));

        let arg = match resolve(&mut db, &args, &bounds, input) {
            TypeRef::Owned(TypeId::ClassInstance(ins)) => {
                ins.type_arguments(&db).get(array_param).unwrap()
            }
            _ => TypeRef::Unknown,
        };

        assert_eq!(arg, owned(instance(string)));
    }

    #[test]
    fn test_generic_trait() {
        let mut db = Database::new();
        let to_foo = new_trait(&mut db, "ToFoo");
        let string = ClassId::string();
        let param = new_parameter(&mut db, "A");
        let trait_param = to_foo.new_type_parameter(&mut db, "T".to_string());
        let args = type_arguments(vec![(param, owned(instance(string)))]);
        let bounds = TypeBounds::new();
        let input = owned(generic_trait_instance_id(
            &mut db,
            to_foo,
            vec![owned(parameter(param))],
        ));

        let arg = match resolve(&mut db, &args, &bounds, input) {
            TypeRef::Owned(TypeId::TraitInstance(ins)) => {
                ins.type_arguments(&db).get(trait_param).unwrap()
            }
            _ => TypeRef::Unknown,
        };

        assert_eq!(arg, owned(instance(string)));
    }

    #[test]
    fn test_closure() {
        let mut db = Database::new();
        let fun = Closure::alloc(&mut db, false);
        let param = new_parameter(&mut db, "T");

        fun.set_return_type(&mut db, owned(parameter(param)));
        fun.new_argument(
            &mut db,
            "a".to_string(),
            owned(rigid(param)),
            infer(parameter(param)),
        );

        let args = type_arguments(vec![(param, TypeRef::Any)]);
        let bounds = TypeBounds::new();
        let output = match resolve(&mut db, &args, &bounds, owned(closure(fun)))
        {
            TypeRef::Owned(TypeId::Closure(id)) => id,
            _ => panic!("Expected the resolved value to be a closure"),
        };

        assert_eq!(output.return_type(&db), TypeRef::Any);
        assert_eq!(output.arguments(&db)[0].value_type, TypeRef::Any);
    }

    #[test]
    fn test_recursive() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "A");
        let var = TypePlaceholder::alloc(&mut db, None);

        var.assign(&db, owned(parameter(param)));

        let args = type_arguments(vec![(param, placeholder(var))]);
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(parameter(param))),
            owned(parameter(param))
        );
    }

    #[test]
    fn test_bounded_parameter() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "A");
        let bound = new_parameter(&mut db, "A");

        bound.set_original(&mut db, param);

        let args = type_arguments(vec![(param, TypeRef::Any)]);
        let bounds = TypeBounds::new();

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(parameter(bound))),
            TypeRef::Any,
        );
    }

    #[test]
    fn test_type_bounds() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "A");
        let bound = new_parameter(&mut db, "A");

        bound.set_original(&mut db, param);

        let args = TypeArguments::new();
        let bounds = type_bounds(vec![(param, bound)]);

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(rigid(param))),
            owned(rigid(bound))
        );

        assert_eq!(
            resolve(&mut db, &args, &bounds, owned(parameter(param))),
            placeholder(TypePlaceholderId(0))
        );

        assert_eq!(TypePlaceholderId(0).required(&db), Some(bound));
    }
}
