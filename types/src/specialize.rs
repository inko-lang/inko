use crate::{
    Class, ClassId, ClassInstance, Database, Shape, TypeId, TypeParameterId,
    TypeRef,
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

    /// The list of classes created during type specialization.
    classes: &'c mut Vec<ClassId>,

    /// A cache of existing shapes to use when encountering a type parameter.
    ///
    /// When specializing a class, it may have fields or variants that are or
    /// contain its type parameter (e.g. `Array[T]` for a `Foo[T]`). When
    /// encountering such types, we need to reuse the shape of the type
    /// parameter as it was determined when creating the newly specialized
    /// class.
    shapes: &'b HashMap<TypeParameterId, Shape>,
}

impl<'a, 'b, 'c> TypeSpecializer<'a, 'b, 'c> {
    pub fn new(
        db: &'a mut Database,
        shapes: &'b HashMap<TypeParameterId, Shape>,
        classes: &'c mut Vec<ClassId>,
    ) -> TypeSpecializer<'a, 'b, 'c> {
        TypeSpecializer { db, shapes, classes }
    }

    pub fn specialize(&mut self, value: TypeRef) -> TypeRef {
        match value {
            // When specializing type parameters, we have to reuse existing
            // shapes if there are any. This leads to a bit of duplication, but
            // there's not really a way around that without making things more
            // complicated than they already are.
            TypeRef::Owned(
                TypeId::TypeParameter(pid) | TypeId::RigidTypeParameter(pid),
            )
            | TypeRef::Any(
                TypeId::TypeParameter(pid) | TypeId::RigidTypeParameter(pid),
            )
            | TypeRef::Uni(
                TypeId::TypeParameter(pid) | TypeId::RigidTypeParameter(pid),
            ) => match self.shapes.get(&pid) {
                Some(Shape::Int) => TypeRef::int(),
                Some(Shape::Float) => TypeRef::float(),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Ref) => value.as_ref(self.db),
                Some(Shape::Mut) => value.force_as_mut(self.db),
                Some(Shape::Atomic) => {
                    TypeRef::Owned(TypeId::AtomicTypeParameter(pid))
                }
                _ => value,
            },
            TypeRef::Ref(
                TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id),
            )
            | TypeRef::UniRef(
                TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id),
            ) => match self.shapes.get(&id) {
                Some(Shape::Int) => TypeRef::int(),
                Some(Shape::Float) => TypeRef::float(),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Atomic) => {
                    TypeRef::Ref(TypeId::AtomicTypeParameter(id))
                }
                _ => value.as_ref(self.db),
            },
            TypeRef::Mut(
                TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id),
            )
            | TypeRef::UniMut(
                TypeId::TypeParameter(id) | TypeId::RigidTypeParameter(id),
            ) => match self.shapes.get(&id) {
                Some(Shape::Int) => TypeRef::int(),
                Some(Shape::Float) => TypeRef::float(),
                Some(Shape::Boolean) => TypeRef::boolean(),
                Some(Shape::String) => TypeRef::string(),
                Some(Shape::Nil) => TypeRef::nil(),
                Some(Shape::Ref) => value.as_ref(self.db),
                Some(Shape::Atomic) => {
                    TypeRef::Mut(TypeId::AtomicTypeParameter(id))
                }
                _ => value.force_as_mut(self.db),
            },

            TypeRef::Owned(id) | TypeRef::Any(id) => {
                TypeRef::Owned(self.specialize_type_id(id))
            }
            TypeRef::Uni(id) => TypeRef::Uni(self.specialize_type_id(id)),

            // Value types should always be specialized as owned types, even
            // when using e.g. `ref Int`.
            TypeRef::Ref(TypeId::ClassInstance(ins))
            | TypeRef::Mut(TypeId::ClassInstance(ins))
            | TypeRef::UniRef(TypeId::ClassInstance(ins))
            | TypeRef::UniMut(TypeId::ClassInstance(ins))
                if ins.instance_of().is_value_type(self.db) =>
            {
                TypeRef::Owned(
                    self.specialize_type_id(TypeId::ClassInstance(ins)),
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

    fn specialize_type_id(&mut self, id: TypeId) -> TypeId {
        match id {
            TypeId::ClassInstance(ins) => {
                let cls = ins.instance_of();

                // For closures we always specialize the classes, based on the
                // assumption that most (if not almost all closures) are likely
                // to capture generic types, and thus any "does this closure
                // capture generics?" check is likely to be true most of the
                // time. Even if it's false, the worst case is that we perform
                // some redundant work.
                if cls.is_generic(self.db) {
                    TypeId::ClassInstance(self.specialize_generic_instance(ins))
                } else if cls.is_closure(self.db) {
                    TypeId::ClassInstance(self.specialize_closure_instance(ins))
                } else {
                    // Regular types may contain generic types in their fields
                    // or variants, so we'll need to update those types.
                    TypeId::ClassInstance(self.specialize_regular_instance(ins))
                }
            }
            _ => id,
        }
    }

    fn specialize_regular_instance(
        &mut self,
        ins: ClassInstance,
    ) -> ClassInstance {
        let class = ins.instance_of();

        if class.specialization_source(self.db).is_some() {
            return ins;
        }

        // Rather than introducing another flag to skip processing already
        // processed types, we just reuse the specialization source.
        class.set_specialization_source(self.db, class);
        self.classes.push(class);

        if class.kind(self.db).is_enum() {
            for var in class.get(self.db).variants.values().clone() {
                let members = var
                    .members(self.db)
                    .into_iter()
                    .map(|v| {
                        TypeSpecializer::new(self.db, self.shapes, self.classes)
                            .specialize(v)
                    })
                    .collect();

                var.set_members(self.db, members);
            }
        }

        for field in class.fields(self.db) {
            let old = field.value_type(self.db);
            let new = TypeSpecializer::new(self.db, self.shapes, self.classes)
                .specialize(old);

            field.set_value_type(self.db, new);
        }

        ins
    }

    fn specialize_generic_instance(
        &mut self,
        ins: ClassInstance,
    ) -> ClassInstance {
        let class = ins.instance_of;

        if class.specialization_source(self.db).is_some() {
            return ins;
        }

        let mut args = ins.type_arguments(self.db).clone();
        let mut key = Vec::new();

        for param in class.type_parameters(self.db) {
            let arg = self.specialize(args.get(param).unwrap());
            let shape = arg.shape(self.db, self.shapes);

            key.push(shape);
            args.assign(param, arg);
        }

        let new = class
            .get(self.db)
            .specializations
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_class(class, key));

        ClassInstance::generic(self.db, new, args)
    }

    fn specialize_closure_instance(
        &mut self,
        ins: ClassInstance,
    ) -> ClassInstance {
        let class = ins.instance_of;

        if class.specialization_source(self.db).is_some() {
            return ins;
        }

        // Closures may capture types that contain generic type parameters. If
        // the shapes of those parameters changes, we must specialize the
        // closure accordingly. For this reason, the specialization key is all
        // the shapes the closure can possibly access, rather than this being
        // limited to the types captured.
        let key = ordered_shapes_from_map(self.shapes);
        let new = class
            .get(self.db)
            .specializations
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_class(class, key));

        ClassInstance::new(new)
    }

    fn specialize_class(&mut self, class: ClassId, key: Vec<Shape>) -> ClassId {
        let (name, kind, vis, module, loc) = {
            let cls = class.get(self.db);
            let loc = class.location(self.db);

            (cls.name.clone(), cls.kind, cls.visibility, cls.module, loc)
        };

        let new = Class::alloc(self.db, name, kind, vis, module, loc);

        self.classes.push(new);
        new.set_specialization_source(self.db, class);

        // We just copy over the type parameters as-is, as there's nothing
        // stored in them that we can't share between the different class
        // specializations.
        for param in class.type_parameters(self.db) {
            let name = param.name(self.db).clone();

            new.get_mut(self.db).type_parameters.insert(name, param);
        }

        new.set_shapes(self.db, key.clone());
        class.get_mut(self.db).specializations.insert(key.clone(), new);

        // When specializing fields and variants, we want them to reuse the
        // shapes we just created.
        let class_mapping = class
            .type_parameters(self.db)
            .into_iter()
            .zip(key)
            .fold(HashMap::new(), |mut map, (param, shape)| {
                map.insert(param, shape);
                map
            });

        // Closures may capture generic parameters from the outside, and the
        // classes themselves aren't generic, so we reuse the outer shapes
        // instead.
        let kind = class.kind(self.db);
        let mapping =
            if kind.is_closure() { self.shapes } else { &class_mapping };

        if kind.is_enum() {
            for old_var in class.get(self.db).variants.values().clone() {
                let name = old_var.name(self.db).clone();
                let loc = old_var.location(self.db).clone();
                let members = old_var
                    .members(self.db)
                    .into_iter()
                    .map(|v| {
                        TypeSpecializer::new(self.db, mapping, self.classes)
                            .specialize(v)
                    })
                    .collect();

                new.new_variant(self.db, name, members, loc);
            }
        }

        for (idx, old_field) in class.fields(self.db).into_iter().enumerate() {
            let (name, orig_typ, vis, module, loc) = {
                let field = old_field.get(self.db);

                (
                    field.name.clone(),
                    field.value_type,
                    field.visibility,
                    field.module,
                    field.location.clone(),
                )
            };

            let typ = TypeSpecializer::new(self.db, mapping, self.classes)
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
        any, generic_instance_id, immutable, instance, mutable, new_enum_class,
        new_parameter, owned, parameter, rigid, uni,
    };
    use crate::{ClassId, Location, ModuleId, Visibility};

    #[test]
    fn test_specialize_type() {
        let mut db = Database::new();
        let class = ClassId::array();
        let shapes = HashMap::new();

        class.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw1 = owned(generic_instance_id(&mut db, class, vec![int]));
        let raw2 = owned(generic_instance_id(&mut db, class, vec![int]));
        let mut classes = Vec::new();
        let spec1 = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw1);
        let spec2 = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw2);

        assert_eq!(format_type(&db, spec1), "Array[Int]");
        assert_eq!(format_type(&db, spec2), "Array[Int]");
        assert_eq!(class.get(&db).specializations.len(), 1);

        let new_class =
            *class.get(&db).specializations.get(&vec![Shape::Int]).unwrap();

        assert_eq!(classes, &[ClassId::int(), new_class]);
        assert_eq!(new_class.specialization_source(&db), Some(class));
        assert_eq!(new_class.kind(&db), class.kind(&db));
        assert_eq!(new_class.get(&db).visibility, class.get(&db).visibility);
        assert_eq!(new_class.module(&db), class.module(&db));

        // This is to test if we reuse the cached results, instead of just
        // creating a new specialized class every time.
        assert!(matches!(
            spec1,
            TypeRef::Owned(TypeId::ClassInstance(ins)) if ins.instance_of == new_class
        ));
        assert!(matches!(
            spec2,
            TypeRef::Owned(TypeId::ClassInstance(ins)) if ins.instance_of == new_class
        ));
    }

    #[test]
    fn test_specialize_pointer_type() {
        let mut db = Database::new();
        let class = ClassId::array();
        let shapes = HashMap::new();

        class.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw =
            TypeRef::Pointer(generic_instance_id(&mut db, class, vec![int]));
        let mut classes = Vec::new();
        let spec = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw);

        assert_eq!(format_type(&db, spec), "Pointer[Array[Int]]");
    }

    #[test]
    fn test_specialize_type_with_ref_value_types() {
        let mut db = Database::new();
        let class = ClassId::array();
        let shapes = HashMap::new();

        class.new_type_parameter(&mut db, "T".to_string());

        let raw = owned(generic_instance_id(
            &mut db,
            class,
            vec![immutable(instance(ClassId::int()))],
        ));
        let mut classes = Vec::new();
        let spec = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw);

        assert_eq!(format_type(&db, spec), "Array[Int]");
        assert_eq!(classes.len(), 2);
    }

    #[test]
    fn test_specialize_class_with_fields() {
        let mut db = Database::new();
        let tup = ClassId::tuple3();
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

        let mut classes = Vec::new();
        let spec = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw);

        assert_eq!(format_type(&db, spec), "(Int, ref X, mut Y: mut)");
        assert_eq!(classes.len(), 2);

        let ins = if let TypeRef::Owned(TypeId::ClassInstance(ins)) = spec {
            ins
        } else {
            panic!("Expected an owned class instance");
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
    fn test_specialize_enum_class() {
        let mut db = Database::new();
        let opt = new_enum_class(&mut db, "Option");
        let opt_param = opt.new_type_parameter(&mut db, "T".to_string());

        opt.new_variant(
            &mut db,
            "Some".to_string(),
            vec![any(parameter(opt_param))],
            Location::default(),
        );

        opt.new_variant(
            &mut db,
            "None".to_string(),
            Vec::new(),
            Location::default(),
        );

        let mut classes = Vec::new();
        let shapes = HashMap::new();
        let raw =
            owned(generic_instance_id(&mut db, opt, vec![TypeRef::int()]));

        let res = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw);

        assert_eq!(classes.len(), 2);
        assert!(classes[1].kind(&db).is_enum());

        let ins = if let TypeRef::Owned(TypeId::ClassInstance(ins)) = res {
            ins
        } else {
            panic!("Expected an owned class instance");
        };

        assert!(ins.instance_of().kind(&db).is_enum());
        assert_eq!(ins.instance_of().shapes(&db), &[Shape::Int]);
        assert_eq!(
            ins.instance_of().variant(&db, "Some").unwrap().members(&db),
            vec![TypeRef::int()]
        );
    }

    #[test]
    fn test_specialize_already_specialized_type() {
        let mut db = Database::new();
        let class = ClassId::array();
        let shapes = HashMap::new();

        class.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let raw = owned(generic_instance_id(&mut db, class, vec![int]));
        let mut classes = Vec::new();
        let res1 = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(raw);

        let res2 = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(res1);

        assert_eq!(res1, res2);
        assert_eq!(classes, &[ClassId::int(), res1.class_id(&db).unwrap()]);
    }

    #[test]
    fn test_specialize_atomic_type_parameter() {
        let mut db = Database::new();
        let mut shapes = HashMap::new();
        let mut classes = Vec::new();
        let param = new_parameter(&mut db, "A");

        shapes.insert(param, Shape::Atomic);

        let owned = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(owned(parameter(param)));

        let immutable = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(immutable(parameter(param)));

        let mutable = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(mutable(parameter(param)));

        assert_eq!(owned, TypeRef::Owned(TypeId::AtomicTypeParameter(param)));
        assert_eq!(immutable, TypeRef::Ref(TypeId::AtomicTypeParameter(param)));
        assert_eq!(mutable, TypeRef::Mut(TypeId::AtomicTypeParameter(param)));
    }

    #[test]
    fn test_specialize_mutable_type_parameter() {
        let mut db = Database::new();
        let mut shapes = HashMap::new();
        let mut classes = Vec::new();
        let param = new_parameter(&mut db, "A");

        shapes.insert(param, Shape::Mut);

        let owned = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(owned(parameter(param)));

        let uni = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(uni(parameter(param)));

        let immutable = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(immutable(parameter(param)));

        let mutable = TypeSpecializer::new(&mut db, &shapes, &mut classes)
            .specialize(mutable(parameter(param)));

        assert_eq!(owned, TypeRef::Mut(TypeId::TypeParameter(param)));
        assert_eq!(uni, TypeRef::UniMut(TypeId::TypeParameter(param)));
        assert_eq!(immutable, TypeRef::Ref(TypeId::TypeParameter(param)));
        assert_eq!(mutable, TypeRef::Mut(TypeId::TypeParameter(param)));
    }
}
