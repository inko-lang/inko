use crate::{
    Database, InternedTypeArguments, Shape, SpecializationKey, TypeEnum,
    TypeId, TypeInstance, TypeParameterId, TypeRef,
};
use std::collections::HashMap;

/// Returns a list of shapes from a shape mapping, sorted by the type parameter
/// IDs.
pub fn ordered_shapes_from_map(
    map: &HashMap<TypeParameterId, Shape>,
) -> Vec<Shape> {
    let mut pairs: Vec<_> = map.iter().collect();

    // Rust HashMaps don't follow a stable order, so we sort by the type
    // parameter IDs to ensure a consistent specialization key.
    pairs.sort_by_key(|(p, _)| p.0);
    pairs.into_iter().map(|(_, s)| *s).collect()
}

/// A type which takes a (potentially) generic type, and specializes it and its
/// fields (if it has any).
///
/// This type handles only type signatures, closure _literals_ are not
/// specialized; instead the compiler does this itself in its specialization
/// pass.
pub struct TypeSpecializer<'a, 'b, 'c> {
    db: &'a mut Database,
    interned: &'b mut InternedTypeArguments,

    /// The list of types created during type specialization.
    types: &'c mut Vec<TypeId>,

    /// A cache of existing shapes to use when encountering a type parameter.
    ///
    /// When specializing a type, it may have fields or constructors that are
    /// or contain its type parameter (e.g. `Array[T]` for a `Foo[T]`). When
    /// encountering such types, we need to reuse the shape of the type
    /// parameter as it was determined when creating the newly specialized
    /// type.
    shapes: &'b HashMap<TypeParameterId, Shape>,

    /// The type `self` is an instance of.
    self_type: TypeEnum,
}

impl<'a, 'b, 'c> TypeSpecializer<'a, 'b, 'c> {
    pub fn specialize_shapes(
        db: &'a mut Database,
        interned: &'b mut InternedTypeArguments,
        shapes: &'b HashMap<TypeParameterId, Shape>,
        types: &'c mut Vec<TypeId>,
        self_type: TypeEnum,
        key: &mut Vec<Shape>,
    ) {
        for shape in key {
            TypeSpecializer::specialize_shape(
                db, interned, shapes, types, self_type, shape,
            );
        }
    }

    pub fn specialize_shape(
        db: &'a mut Database,
        interned: &'b mut InternedTypeArguments,
        shapes: &'b HashMap<TypeParameterId, Shape>,
        types: &'c mut Vec<TypeId>,
        self_type: TypeEnum,
        shape: &mut Shape,
    ) {
        match shape {
            Shape::Copy(i)
            | Shape::Inline(i)
            | Shape::InlineRef(i)
            | Shape::InlineMut(i) => {
                *i = TypeSpecializer::new(
                    db, interned, shapes, types, self_type,
                )
                .specialize_type_instance(*i);
            }
            _ => {}
        }
    }

    pub fn new(
        db: &'a mut Database,
        interned: &'b mut InternedTypeArguments,
        shapes: &'b HashMap<TypeParameterId, Shape>,
        types: &'c mut Vec<TypeId>,
        self_type: TypeEnum,
    ) -> TypeSpecializer<'a, 'b, 'c> {
        TypeSpecializer { db, interned, shapes, types, self_type }
    }

    pub fn specialize(&mut self, value: TypeRef) -> TypeRef {
        match value {
            // When specializing default methods inherited from traits, we need
            // to replace the trait types used for `self` with the type of
            // whatever implements the trait. This is needed such that if e.g. a
            // closure captures `self` and `self` is a stack allocated type, the
            // closure is specialized correctly.
            TypeRef::Owned(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type)
            }
            TypeRef::Uni(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Uni(self.self_type)
            }
            TypeRef::Ref(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Ref(self.self_type)
            }
            TypeRef::Mut(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Mut(self.self_type)
            }
            TypeRef::UniRef(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::UniRef(self.self_type)
            }
            TypeRef::UniMut(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::UniMut(self.self_type)
            }
            // When specializing type parameters, we have to reuse existing
            // shapes if there are any. This leads to a bit of duplication, but
            // there's not really a way around that without making things more
            // complicated than they already are.
            TypeRef::Owned(
                TypeEnum::TypeParameter(pid)
                | TypeEnum::RigidTypeParameter(pid),
            )
            | TypeRef::Any(
                TypeEnum::TypeParameter(pid)
                | TypeEnum::RigidTypeParameter(pid),
            )
            | TypeRef::Uni(
                TypeEnum::TypeParameter(pid)
                | TypeEnum::RigidTypeParameter(pid),
            ) => match self.shapes.get(&pid) {
                Some(&Shape::Int(size, sign)) => {
                    TypeRef::int_with_sign(size, sign)
                }
                Some(&Shape::Float(s)) => TypeRef::float_with_size(s),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Ref) => value.as_ref(self.db),
                Some(Shape::Mut) => value.force_as_mut(self.db),
                Some(Shape::Atomic) => {
                    TypeRef::Owned(TypeEnum::AtomicTypeParameter(pid))
                }
                Some(Shape::Inline(i) | Shape::Copy(i)) => TypeRef::Owned(
                    TypeEnum::TypeInstance(self.specialize_type_instance(*i)),
                ),
                Some(Shape::InlineRef(i)) => TypeRef::Ref(
                    TypeEnum::TypeInstance(self.specialize_type_instance(*i)),
                ),
                Some(Shape::InlineMut(i)) => TypeRef::Mut(
                    TypeEnum::TypeInstance(self.specialize_type_instance(*i)),
                ),
                _ => value,
            },
            TypeRef::Ref(
                TypeEnum::TypeParameter(id) | TypeEnum::RigidTypeParameter(id),
            )
            | TypeRef::UniRef(
                TypeEnum::TypeParameter(id) | TypeEnum::RigidTypeParameter(id),
            ) => match self.shapes.get(&id) {
                Some(&Shape::Int(size, sign)) => {
                    TypeRef::int_with_sign(size, sign)
                }
                Some(&Shape::Float(s)) => TypeRef::float_with_size(s),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Atomic) => {
                    TypeRef::Ref(TypeEnum::AtomicTypeParameter(id))
                }
                Some(Shape::Copy(i)) => TypeRef::Owned(TypeEnum::TypeInstance(
                    self.specialize_type_instance(*i),
                )),
                Some(
                    Shape::Inline(i)
                    | Shape::InlineRef(i)
                    | Shape::InlineMut(i),
                ) => TypeRef::Ref(TypeEnum::TypeInstance(
                    self.specialize_type_instance(*i),
                )),
                _ => value.as_ref(self.db),
            },
            TypeRef::Mut(
                TypeEnum::TypeParameter(id) | TypeEnum::RigidTypeParameter(id),
            )
            | TypeRef::UniMut(
                TypeEnum::TypeParameter(id) | TypeEnum::RigidTypeParameter(id),
            ) => match self.shapes.get(&id) {
                Some(&Shape::Int(size, sign)) => {
                    TypeRef::int_with_sign(size, sign)
                }
                Some(&Shape::Float(s)) => TypeRef::float_with_size(s),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Ref) => value.as_ref(self.db),
                Some(Shape::Atomic) => {
                    TypeRef::Mut(TypeEnum::AtomicTypeParameter(id))
                }
                Some(Shape::Copy(i)) => TypeRef::Owned(TypeEnum::TypeInstance(
                    self.specialize_type_instance(*i),
                )),
                Some(Shape::InlineRef(i)) => TypeRef::Ref(
                    TypeEnum::TypeInstance(self.specialize_type_instance(*i)),
                ),
                Some(Shape::Inline(i) | Shape::InlineMut(i)) => TypeRef::Mut(
                    TypeEnum::TypeInstance(self.specialize_type_instance(*i)),
                ),
                _ => value.force_as_mut(self.db),
            },
            TypeRef::Owned(id) | TypeRef::Any(id) => {
                TypeRef::Owned(self.specialize_type_id(id))
            }
            TypeRef::Uni(id) => TypeRef::Uni(self.specialize_type_id(id)),
            // Value types should always be specialized as owned types, even
            // when using e.g. `ref Int`.
            TypeRef::Ref(TypeEnum::TypeInstance(ins))
            | TypeRef::Mut(TypeEnum::TypeInstance(ins))
            | TypeRef::UniRef(TypeEnum::TypeInstance(ins))
            | TypeRef::UniMut(TypeEnum::TypeInstance(ins))
                if ins.instance_of().is_value_type(self.db) =>
            {
                TypeRef::Owned(
                    self.specialize_type_id(TypeEnum::TypeInstance(ins)),
                )
            }
            TypeRef::Ref(id) => TypeRef::Ref(self.specialize_type_id(id)),
            TypeRef::Mut(id) => TypeRef::Mut(self.specialize_type_id(id)),
            TypeRef::UniRef(id) => TypeRef::UniRef(self.specialize_type_id(id)),
            TypeRef::UniMut(id) => TypeRef::UniMut(self.specialize_type_id(id)),
            TypeRef::Placeholder(id) => {
                id.value(self.db).map_or(value, |v| self.specialize(v))
            }
            TypeRef::Pointer(id) => {
                TypeRef::Pointer(self.specialize_type_id(id))
            }
            _ => value,
        }
    }

    fn specialize_type_id(&mut self, id: TypeEnum) -> TypeEnum {
        if let TypeEnum::TypeInstance(ins) = id {
            TypeEnum::TypeInstance(self.specialize_type_instance(ins))
        } else {
            id
        }
    }

    pub fn specialize_type_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        let cls = ins.instance_of();

        // For closures we always specialize the types, based on the
        // assumption that most (if not almost all closures) are likely to
        // capture generic types, and thus any "does this closure capture
        // generics?" check is likely to be true most of the time. Even if it's
        // false, the worst case is that we perform some redundant work.
        if cls.is_generic(self.db) {
            self.specialize_generic_instance(ins)
        } else if cls.is_closure(self.db) {
            self.specialize_closure_instance(ins)
        } else {
            // Regular types may contain generic types in their fields or
            // constructors, so we'll need to update those types.
            self.specialize_regular_instance(ins)
        }
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn specialize_regular_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        let typ = ins.instance_of();

        // For regular instances we only need to specialize the first reference.
        if typ.specialization_source(self.db).is_some() {
            return ins;
        }

        typ.set_specialization_source(self.db, typ);
        self.types.push(typ);

        if typ.kind(self.db).is_enum() {
            for var in typ.constructors(self.db) {
                let args = var
                    .arguments(self.db)
                    .to_vec()
                    .into_iter()
                    .map(|v| {
                        TypeSpecializer::new(
                            self.db,
                            self.interned,
                            self.shapes,
                            self.types,
                            self.self_type,
                        )
                        .specialize(v)
                    })
                    .collect();

                var.set_arguments(self.db, args);
            }
        }

        for field in typ.fields(self.db) {
            let old = field.value_type(self.db);
            let new = TypeSpecializer::new(
                self.db,
                self.interned,
                self.shapes,
                self.types,
                self.self_type,
            )
            .specialize(old);

            field.set_value_type(self.db, new);
        }

        ins
    }

    fn specialize_generic_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        let typ = ins.instance_of;

        if typ.specialization_source(self.db).is_some() {
            return ins;
        }

        let mut args = ins.type_arguments(self.db).unwrap().clone();
        let mut shapes: Vec<Shape> = typ
            .type_parameters(self.db)
            .into_iter()
            .map(|p| {
                let raw = args.get(p).unwrap();
                let typ = self.specialize(raw);

                args.assign(p, typ);
                typ.shape(self.db, self.interned, self.shapes)
            })
            .collect();

        TypeSpecializer::specialize_shapes(
            self.db,
            self.interned,
            self.shapes,
            self.types,
            self.self_type,
            &mut shapes,
        );

        let key = SpecializationKey::new(shapes);
        let new = typ
            .specializations(self.db)
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_type(typ, key));

        // We keep the type arguments so we can perform type checking where
        // necessary during specialization (e.g. when checking if a stack type
        // implements a trait).
        TypeInstance::generic(self.db, new, args)
    }

    fn specialize_closure_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        // We don't check the specialization source for closures, as each
        // closure _always_ needs to be specialized, as its behaviour/layout may
        // change based on how the surrounding method is specialized.
        //
        // Closures may capture types that contain generic type parameters. If
        // the shapes of those parameters changes, we must specialize the
        // closure accordingly. For this reason, the specialization key is all
        // the shapes the closure can possibly access, rather than this being
        // limited to the types captured.
        let mut shapes = ordered_shapes_from_map(self.shapes);

        TypeSpecializer::specialize_shapes(
            self.db,
            self.interned,
            self.shapes,
            self.types,
            self.self_type,
            &mut shapes,
        );

        let key = SpecializationKey::for_closure(self.self_type, shapes);
        let typ = ins.instance_of;
        let new = typ
            .specializations(self.db)
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_type(typ, key));

        TypeInstance::new(new)
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn specialize_type(
        &mut self,
        type_id: TypeId,
        key: SpecializationKey,
    ) -> TypeId {
        let new = type_id.clone_for_specialization(self.db);

        self.types.push(new);
        new.set_specialization_source(self.db, type_id);

        // We just copy over the type parameters as-is, as there's nothing
        // stored in them that we can't share between the different type
        // specializations.
        for param in type_id.type_parameters(self.db) {
            let name = param.name(self.db).clone();

            new.get_mut(self.db).type_parameters.insert(name, param);
        }

        type_id.add_specialization(self.db, key, new);

        // When specializing fields and constructors, we want them to reuse the
        // shapes we just created.
        let mut type_mapping = HashMap::new();

        // Closures may capture generic parameters from the outside, and the
        // types themselves aren't generic, so we reuse the outer shapes
        // instead.
        let kind = type_id.kind(self.db);
        let mapping = if kind.is_closure() {
            self.shapes
        } else {
            for (param, &shape) in type_id
                .type_parameters(self.db)
                .into_iter()
                .zip(new.shapes(self.db))
            {
                type_mapping.insert(param, shape);
            }

            &type_mapping
        };

        if kind.is_enum() {
            for old_cons in type_id.constructors(self.db) {
                let name = old_cons.name(self.db).clone();
                let loc = old_cons.location(self.db);
                let args = old_cons
                    .arguments(self.db)
                    .to_vec()
                    .into_iter()
                    .map(|v| {
                        TypeSpecializer::new(
                            self.db,
                            self.interned,
                            mapping,
                            self.types,
                            self.self_type,
                        )
                        .specialize(v)
                    })
                    .collect();

                new.new_constructor(self.db, name, args, loc);
            }
        }

        for (idx, old_field) in type_id.fields(self.db).into_iter().enumerate()
        {
            let (name, orig_typ, vis, module, loc) = {
                let field = old_field.get(self.db);

                (
                    field.name.clone(),
                    field.value_type,
                    field.visibility,
                    field.module,
                    field.location,
                )
            };

            let typ = TypeSpecializer::new(
                self.db,
                self.interned,
                mapping,
                self.types,
                self.self_type,
            )
            .specialize(orig_typ);

            new.new_field(self.db, name, idx as _, typ, vis, module, loc);
        }

        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::format_type;
    use crate::test::{
        any, generic_instance_id, immutable, instance, mutable, new_enum_type,
        new_parameter, new_trait, new_type, owned, parameter, rigid, uni,
    };
    use crate::{Location, ModuleId, TraitInstance, TypeId, Visibility};

    #[test]
    fn test_specialize_type() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let ary = TypeId::array();
        let shapes = HashMap::new();

        ary.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw1 = owned(generic_instance_id(&mut db, ary, vec![int]));
        let raw2 = owned(generic_instance_id(&mut db, ary, vec![int]));
        let mut types = Vec::new();
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let spec1 = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw1);
        let spec2 = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw2);

        assert_eq!(format_type(&db, spec1), "Array[Int]");
        assert_eq!(format_type(&db, spec2), "Array[Int]");
        assert_eq!(ary.specializations(&db).len(), 1);

        let key = SpecializationKey::new(vec![Shape::int()]);
        let new_type = *ary.specializations(&db).get(&key).unwrap();

        assert_eq!(types, &[TypeId::int(), new_type]);
        assert_eq!(new_type.specialization_source(&db), Some(ary));
        assert_eq!(new_type.kind(&db), ary.kind(&db));
        assert_eq!(new_type.get(&db).visibility, ary.get(&db).visibility);
        assert_eq!(new_type.module(&db), ary.module(&db));

        // This is to test if we reuse the cached results, instead of just
        // creating a new specialized type every time.
        assert!(matches!(
            spec1,
            TypeRef::Owned(TypeEnum::TypeInstance(ins)) if ins.instance_of == new_type
        ));
        assert!(matches!(
            spec2,
            TypeRef::Owned(TypeEnum::TypeInstance(ins)) if ins.instance_of == new_type
        ));
    }

    #[test]
    fn test_specialize_pointer_type() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let ary = TypeId::array();
        let shapes = HashMap::new();

        ary.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw =
            TypeRef::Pointer(generic_instance_id(&mut db, ary, vec![int]));
        let mut types = Vec::new();
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let spec = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw);

        assert_eq!(format_type(&db, spec), "Pointer[Array[Int]]");
    }

    #[test]
    fn test_specialize_type_with_ref_value_types() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let foo = new_type(&mut db, "Foo");
        let ary = TypeId::array();
        let shapes = HashMap::new();

        ary.new_type_parameter(&mut db, "T".to_string());

        let raw = owned(generic_instance_id(
            &mut db,
            ary,
            vec![immutable(instance(foo))],
        ));
        let mut types = Vec::new();
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let spec = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw);

        assert_eq!(format_type(&db, spec), "Array[ref Foo]");
        assert_eq!(
            spec,
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance {
                instance_of: TypeId(db.number_of_types() as u32 - 1),
                type_arguments: 1,
            }))
        );
        assert_eq!(types.len(), 2);
    }

    #[test]
    fn test_specialize_type_with_fields() {
        let mut db = Database::new();
        let tup = TypeId::tuple3();
        let param1 = tup.new_type_parameter(&mut db, "A".to_string());
        let param2 = tup.new_type_parameter(&mut db, "B".to_string());
        let param3 = tup.new_type_parameter(&mut db, "C".to_string());

        param3.set_mutable(&mut db);

        let rigid1 = new_parameter(&mut db, "X");
        let rigid2 = new_parameter(&mut db, "Y");

        rigid2.set_mutable(&mut db);

        tup.new_field(
            &mut db,
            "0".to_string(),
            0,
            any(parameter(param1)),
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        tup.new_field(
            &mut db,
            "1".to_string(),
            1,
            any(parameter(param2)),
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        tup.new_field(
            &mut db,
            "2".to_string(),
            2,
            any(parameter(param3)),
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        let mut shapes = HashMap::new();

        shapes.insert(rigid1, Shape::Owned);
        shapes.insert(rigid2, Shape::Owned);

        let raw = owned(generic_instance_id(
            &mut db,
            tup,
            vec![
                TypeRef::int(),
                immutable(rigid(rigid1)),
                mutable(rigid(rigid2)),
            ],
        ));

        let mut interned = InternedTypeArguments::new();
        let mut types = Vec::new();
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let spec = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw);

        assert_eq!(format_type(&db, spec), "(Int, ref X, mut Y: mut)");
        assert_eq!(types.len(), 2);

        let ins = if let TypeRef::Owned(TypeEnum::TypeInstance(ins)) = spec {
            ins
        } else {
            panic!("Expected an owned type instance");
        };

        assert_ne!(ins.instance_of(), tup);
        assert!(ins.instance_of().kind(&db).is_tuple());
        assert_eq!(
            ins.instance_of().field_by_index(&db, 0).unwrap().value_type(&db),
            TypeRef::int(),
        );

        assert_eq!(
            ins.instance_of().field_by_index(&db, 1).unwrap().value_type(&db),
            immutable(parameter(param2)),
        );

        assert_eq!(
            ins.instance_of().field_by_index(&db, 2).unwrap().value_type(&db),
            mutable(parameter(param3)),
        );
    }

    #[test]
    fn test_specialize_enum_type() {
        let mut db = Database::new();
        let opt = new_enum_type(&mut db, "Option");
        let opt_param = opt.new_type_parameter(&mut db, "T".to_string());

        opt.new_constructor(
            &mut db,
            "Some".to_string(),
            vec![any(parameter(opt_param))],
            Location::default(),
        );

        opt.new_constructor(
            &mut db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );

        let mut interned = InternedTypeArguments::new();
        let mut types = Vec::new();
        let shapes = HashMap::new();
        let raw =
            owned(generic_instance_id(&mut db, opt, vec![TypeRef::int()]));

        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let res = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw);

        assert_eq!(types.len(), 2);
        assert!(types[1].kind(&db).is_enum());

        let ins = if let TypeRef::Owned(TypeEnum::TypeInstance(ins)) = res {
            ins
        } else {
            panic!("Expected an owned type instance");
        };

        assert!(ins.instance_of().kind(&db).is_enum());
        assert_eq!(ins.instance_of().shapes(&db), &[Shape::int()]);
        assert_eq!(
            ins.instance_of().constructor(&db, "Some").unwrap().arguments(&db),
            vec![TypeRef::int()]
        );
    }

    #[test]
    fn test_specialize_already_specialized_type() {
        let mut db = Database::new();
        let ary = TypeId::array();
        let shapes = HashMap::new();

        ary.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw = owned(generic_instance_id(&mut db, ary, vec![int]));
        let mut types = Vec::new();
        let mut interned = InternedTypeArguments::new();
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let res1 = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(raw);

        let res2 = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(res1);

        assert_eq!(res1, res2);
        assert_eq!(types, &[TypeId::int(), res1.type_id(&db).unwrap()]);
    }

    #[test]
    fn test_specialize_atomic_type_parameter() {
        let mut db = Database::new();
        let mut shapes = HashMap::new();
        let mut types = Vec::new();
        let mut interned = InternedTypeArguments::new();
        let param = new_parameter(&mut db, "A");

        shapes.insert(param, Shape::Atomic);

        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let owned = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(owned(parameter(param)));

        let immutable = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(immutable(parameter(param)));

        let mutable = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(mutable(parameter(param)));

        assert_eq!(owned, TypeRef::Owned(TypeEnum::AtomicTypeParameter(param)));
        assert_eq!(
            immutable,
            TypeRef::Ref(TypeEnum::AtomicTypeParameter(param))
        );
        assert_eq!(mutable, TypeRef::Mut(TypeEnum::AtomicTypeParameter(param)));
    }

    #[test]
    fn test_specialize_mutable_type_parameter() {
        let mut db = Database::new();
        let mut shapes = HashMap::new();
        let mut types = Vec::new();
        let mut interned = InternedTypeArguments::new();
        let param = new_parameter(&mut db, "A");

        shapes.insert(param, Shape::Mut);

        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));
        let owned = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(owned(parameter(param)));

        let uni = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(uni(parameter(param)));

        let immutable = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(immutable(parameter(param)));

        let mutable = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            stype,
        )
        .specialize(mutable(parameter(param)));

        assert_eq!(owned, TypeRef::Mut(TypeEnum::TypeParameter(param)));
        assert_eq!(uni, TypeRef::UniMut(TypeEnum::TypeParameter(param)));
        assert_eq!(immutable, TypeRef::Ref(TypeEnum::TypeParameter(param)));
        assert_eq!(mutable, TypeRef::Mut(TypeEnum::TypeParameter(param)));
    }

    #[test]
    fn test_specialize_borrow_inline_type_parameter() {
        let mut db = Database::new();
        let mut shapes = HashMap::new();
        let mut types = Vec::new();
        let mut interned = InternedTypeArguments::new();
        let cls = new_type(&mut db, "A");
        let ins = TypeInstance::new(cls);
        let p1 = new_parameter(&mut db, "X");
        let p2 = new_parameter(&mut db, "Y");
        let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()));

        shapes.insert(p1, Shape::Inline(ins));
        shapes.insert(p2, Shape::Copy(ins));

        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(owned(parameter(p1))),
            owned(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(uni(parameter(p1))),
            owned(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(mutable(parameter(p1))),
            mutable(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(immutable(parameter(p1))),
            immutable(instance(cls))
        );

        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(owned(parameter(p2))),
            owned(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(owned(parameter(p2))),
            owned(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(mutable(parameter(p2))),
            owned(instance(cls))
        );
        assert_eq!(
            TypeSpecializer::new(
                &mut db,
                &mut interned,
                &shapes,
                &mut types,
                stype
            )
            .specialize(immutable(parameter(p2))),
            owned(instance(cls))
        );
    }

    #[test]
    fn test_specialize_trait_self_type() {
        let mut db = Database::new();
        let shapes = HashMap::new();
        let mut types = Vec::new();
        let mut interned = InternedTypeArguments::new();
        let trt = new_trait(&mut db, "ToThing");
        let cls = new_type(&mut db, "Thing");
        let mut old_self = TraitInstance::new(trt);

        old_self.self_type = true;

        let new_self = TypeEnum::TypeInstance(TypeInstance::new(cls));
        let mut spec = TypeSpecializer::new(
            &mut db,
            &mut interned,
            &shapes,
            &mut types,
            new_self,
        );

        assert_eq!(
            spec.specialize(owned(TypeEnum::TraitInstance(old_self))),
            owned(new_self)
        );
        assert_eq!(
            spec.specialize(immutable(TypeEnum::TraitInstance(old_self))),
            immutable(new_self)
        );
        assert_eq!(
            spec.specialize(mutable(TypeEnum::TraitInstance(old_self))),
            mutable(new_self)
        );
        assert_eq!(
            spec.specialize(uni(TypeEnum::TraitInstance(old_self))),
            uni(new_self)
        );
    }
}
