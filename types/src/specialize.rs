use crate::{
    Block, ClosureId, Database, ForeignType, InternedTypeArguments, Sign,
    SpecializationKey, TraitId, TraitInstance, TypeArguments, TypeEnum, TypeId,
    TypeInstance, TypeParameterId, TypeRef,
};
use std::collections::HashMap;

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct ClosureKey {
    moving: bool,
    arguments: Vec<TypeRef>,
    returns: TypeRef,
}

/// A mapping of the structure of each closure type to the closure ID to use.
///
/// This structure is used to ensure that different closure types with the same
/// structure also translate/specialize to the same exact closure type. This in
/// turn is required to ensure different closures only result in a single
/// specialization (e.g. of a generic method), intsead of one specialization for
/// every closure type.
pub struct Closures {
    mapping: HashMap<ClosureKey, ClosureId>,
}

impl Closures {
    pub fn new() -> Self {
        Self { mapping: HashMap::new() }
    }

    fn get(&self, key: &ClosureKey) -> Option<ClosureId> {
        self.mapping.get(key).cloned()
    }

    fn add(&mut self, key: ClosureKey, id: ClosureId) {
        self.mapping.insert(key, id);
    }
}

/// A type which takes a (potentially) generic type, and specializes it and its
/// fields (if it has any).
pub struct TypeSpecializer<'a> {
    db: &'a mut Database,
    interned: &'a mut InternedTypeArguments,

    /// A mapping of closure structures to an interned closure.
    closures: &'a mut Closures,

    /// The type arguments of the method call.
    arguments: &'a TypeArguments,

    /// The type arguments of the surrounding method.
    surrounding_arguments: &'a TypeArguments,

    /// The list of types created during type specialization.
    types: &'a mut Vec<TypeId>,

    /// The type `self` is an instance of.
    self_type: TypeEnum,
}

impl<'a> TypeSpecializer<'a> {
    pub fn new(
        db: &'a mut Database,
        closures: &'a mut Closures,
        interned: &'a mut InternedTypeArguments,
        type_arguments: &'a TypeArguments,
        surrounding_type_arguments: &'a TypeArguments,
        types: &'a mut Vec<TypeId>,
        self_type: TypeEnum,
    ) -> TypeSpecializer<'a> {
        TypeSpecializer {
            db,
            closures,
            interned,
            arguments: type_arguments,
            surrounding_arguments: surrounding_type_arguments,
            types,
            self_type,
        }
    }

    pub fn specialize(&mut self, value: TypeRef) -> TypeRef {
        match value {
            // When encountering a `Self` type we need to replace it with the
            // actual type to use for the current method.
            TypeRef::Owned(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type)
            }
            TypeRef::Uni(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type).as_uni(self.db)
            }
            TypeRef::Ref(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type).as_ref(self.db)
            }
            TypeRef::Mut(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type).as_mut(self.db)
            }
            TypeRef::UniRef(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type).as_uni_ref(self.db)
            }
            TypeRef::UniMut(TypeEnum::TraitInstance(i)) if i.self_type => {
                TypeRef::Owned(self.self_type).as_uni_mut(self.db)
            }

            // Type parameters are remapped according to the type arguments of
            // the surrounding method.
            TypeRef::Owned(TypeEnum::RigidTypeParameter(p))
            | TypeRef::Any(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p)
            }
            TypeRef::Uni(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p).as_uni(self.db)
            }
            TypeRef::Ref(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p).as_ref(self.db)
            }
            TypeRef::Mut(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p).as_mut(self.db)
            }
            TypeRef::UniRef(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p).as_uni_ref(self.db)
            }
            TypeRef::UniMut(TypeEnum::RigidTypeParameter(p)) => {
                self.specialize_rigid_parameter(p).as_uni_mut(self.db)
            }

            TypeRef::Owned(TypeEnum::TypeParameter(p))
            | TypeRef::Any(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p)
            }
            TypeRef::Uni(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p).as_uni(self.db)
            }
            TypeRef::Ref(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p).as_ref(self.db)
            }
            TypeRef::Mut(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p).as_mut(self.db)
            }
            TypeRef::UniRef(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p).as_uni_ref(self.db)
            }
            TypeRef::UniMut(TypeEnum::TypeParameter(p)) => {
                self.specialize_parameter(p).as_uni_mut(self.db)
            }

            // For other types (e.g. `Array[String]`) we need to intern the
            // generic type arguments.
            TypeRef::Owned(t) | TypeRef::Any(t) => {
                TypeRef::Owned(self.specialize_type_enum(t))
            }

            // Value types should always be specialized as owned types, even
            // when using e.g. `ref Int`.
            TypeRef::Uni(TypeEnum::TypeInstance(ins))
            | TypeRef::Ref(TypeEnum::TypeInstance(ins))
            | TypeRef::Mut(TypeEnum::TypeInstance(ins))
            | TypeRef::UniRef(TypeEnum::TypeInstance(ins))
            | TypeRef::UniMut(TypeEnum::TypeInstance(ins))
                if ins.instance_of().is_value_type(self.db) =>
            {
                TypeRef::Owned(
                    self.specialize_type_enum(TypeEnum::TypeInstance(ins)),
                )
            }
            TypeRef::Uni(t) => TypeRef::Uni(self.specialize_type_enum(t)),
            TypeRef::Ref(t) => TypeRef::Ref(self.specialize_type_enum(t)),
            TypeRef::Mut(t) => TypeRef::Mut(self.specialize_type_enum(t)),
            TypeRef::UniRef(t) => TypeRef::UniRef(self.specialize_type_enum(t)),
            TypeRef::UniMut(t) => TypeRef::UniMut(self.specialize_type_enum(t)),
            TypeRef::Pointer(t) => {
                TypeRef::Pointer(self.specialize_type_enum(t))
            }
            // Placeholders are replaced with their types or a fallback type,
            // such that interning them produces consistent results (as
            // different placeholders have different IDs, even if assigned the
            // exact same type).
            TypeRef::Placeholder(t) => t
                .value(self.db)
                .map_or(TypeRef::Unknown, |v| self.specialize(v)),
            TypeRef::Never | TypeRef::Error | TypeRef::Unknown => value,
        }
    }

    fn specialize_rigid_parameter(&mut self, pid: TypeParameterId) -> TypeRef {
        if let Some(init) = self
            .arguments
            .get(pid)
            .or_else(|| self.surrounding_arguments.get(pid))
        {
            if let TypeRef::Placeholder(p) = init {
                // At this point all placeholders are expected to be assigned a
                // value, otherwise an error during type checking would've been
                // produced.
                let val = p.value(self.db).unwrap();

                // If the placeholder is assigned a parameter, then the
                // parameter must originate from the outer type arguments,
                // because given a call C its method type parameters (if any)
                // can only be assigned types passed in as arguments (i.e. from
                // the outside).
                //
                // Outer arguments in turn are already specialized (ignoring
                // bugs that cause this to not be the case), so we can just
                // return any found value as-is.
                if let Some(outer) = val
                    .as_type_parameter(self.db)
                    .and_then(|t| self.surrounding_arguments.get(t))
                {
                    return outer;
                }

                return val;
            }

            // If the initial parameter is assigned another type parameter then
            // it must come from the surrounding type arguments (similar to how
            // we handle placeholders), and the type is already specialized.
            if let Some(v) = init
                .as_type_parameter(self.db)
                .and_then(|p| self.surrounding_arguments.get(p))
            {
                return v;
            }

            return init;
        }

        // We never reach this point unless there's a bug in the compiler.
        unreachable!(
            "the rigid parameter {} (ID: {}) must be assigned a type
call arguments:
  {:?}
surrounding arguments:
  {:?}",
            pid.name(self.db),
            pid.0,
            self.arguments,
            self.surrounding_arguments,
        );
    }

    fn specialize_parameter(&mut self, pid: TypeParameterId) -> TypeRef {
        if let Some(typ) = self.arguments.get_recursive(self.db, pid) {
            // Regular type parameters may be assigned types that have yet to be
            // specialized (e.g. when processing type parameters referred to by
            // an enum constructor argument), unlike rigit type parameters.
            return self.specialize(typ);
        }

        // We never reach this point unless there's a bug in the compiler.
        unreachable!(
            "the parameter {} (ID: {}) must be assigned a type
call arguments:
  {:?}",
            pid.name(self.db),
            pid.0,
            self.arguments,
        );
    }

    fn specialize_type_enum(&mut self, typ: TypeEnum) -> TypeEnum {
        match typ {
            TypeEnum::TypeInstance(ins) => {
                TypeEnum::TypeInstance(self.specialize_type_instance(ins))
            }
            TypeEnum::Closure(id) => {
                TypeEnum::Closure(self.specialize_closure_type(id))
            }
            TypeEnum::TraitInstance(ins) => {
                TypeEnum::TraitInstance(self.specialize_trait_instance(ins))
            }
            // Int64 needs to be normalized such that it's exactly the same type
            // as just Int, otherwise e.g. `[10]` and `[10 as Int64]` produce
            // two different specializations instead of a single one.
            TypeEnum::Foreign(ForeignType::Int(64, Sign::Signed)) => {
                TypeEnum::TypeInstance(TypeInstance::new(TypeId::int()))
            }
            _ => typ,
        }
    }

    fn specialize_type_instance(&mut self, ins: TypeInstance) -> TypeInstance {
        let typ = ins.instance_of();

        if typ.is_generic(self.db) {
            self.specialize_generic_instance(ins)
        } else if typ.is_closure(self.db) {
            self.specialize_closure_literal(ins)
        } else {
            self.specialize_regular_instance(ins)
        }
    }

    fn specialize_generic_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        let typ = ins.instance_of();

        if typ.specialization_source(self.db).is_some() {
            return ins;
        }

        // The type arguments may contain generic types or refer to type
        // parameters from the surrounding method or type, so we need to
        // specialize them first _before_ interning them.
        //
        // This requires allocating a new TypeArguments in the type database
        // such that we can use it as part of the interning process. We _can't_
        // update the existing type arguments in-place because it's re-used by
        // different yet-to-specialize methods.
        let mut args = ins.type_arguments(self.db).unwrap().clone();

        for typ in args.values_mut() {
            *typ = self.specialize(*typ);
        }

        let ins = ins.with_new_type_arguments(self.db, args);
        let ins = ins.interned(self.db, self.interned);
        let key = SpecializationKey::new(ins.type_arguments(self.db).unwrap());
        let new = typ
            .specializations(self.db)
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_type(typ, key, ins));

        TypeInstance::new(new)
    }

    fn specialize_closure_type(&mut self, old: ClosureId) -> ClosureId {
        if old.specialization_source(self.db).is_some() {
            return old;
        }

        let key = ClosureKey {
            moving: old.captures_by_moving(self.db),
            arguments: old
                .arguments(self.db)
                .into_iter()
                .map(|a| self.specialize(a.value_type))
                .collect(),
            returns: self.specialize(old.return_type(self.db)),
        };

        if let Some(typ) = self.closures.get(&key) {
            return typ;
        }

        let new = old.clone_for_specialization(self.db);

        // Closure _types_ (i.e. signatures) can't capture variables, only
        // closure _literals_ can (which are specialized separately). Thus, we
        // only need to concern ourselves with the argument and return types.
        for &typ in &key.arguments {
            new.new_anonymous_argument(self.db, typ);
        }

        new.set_return_type(self.db, key.returns);
        new.set_specialization_source(self.db, old);
        self.closures.add(key, new);
        new
    }

    fn specialize_closure_literal(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        // We don't check the specialization source for closures, as each
        // closure _always_ needs to be specialized, as its behaviour/layout may
        // change based on how the surrounding method is specialized.
        //
        // Closures don't have generic type arguments, so no interning is
        // needed. We use the surrounding type arguments for the key such that
        // if the surrounding method is specialized multiple times, we generate
        // a new closure for each specialization.
        let stype = self.self_type;
        let key = SpecializationKey::for_closure(stype, self.arguments);
        let typ = ins.instance_of();

        if let Some(typ) = typ.specializations(self.db).get(&key).cloned() {
            return TypeInstance::new(typ);
        }

        let new = self.specialize_type(typ, key, ins);

        new.set_self_type_for_closure(self.db, stype.into());
        TypeInstance::new(new)
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn specialize_regular_instance(
        &mut self,
        ins: TypeInstance,
    ) -> TypeInstance {
        let typ = ins.instance_of();

        if typ.specialization_source(self.db).is_some() {
            return ins;
        }

        typ.set_specialization_source(self.db, typ);
        self.types.push(typ);

        // For enums the constructor argument types are used to determine the
        // layout/size of the enum, so we need to specialize these.
        if typ.kind(self.db).is_enum() {
            for var in typ.constructors(self.db) {
                let args = var
                    .arguments(self.db)
                    .to_vec()
                    .into_iter()
                    .map(|v| self.specialize(v))
                    .collect();

                var.set_arguments(self.db, args);
            }
        }

        for field in typ.fields(self.db) {
            let old = field.value_type(self.db);
            let new = self.specialize(old);

            field.set_value_type(self.db, new);
        }

        ins
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn specialize_type(
        &mut self,
        old: TypeId,
        key: SpecializationKey,
        instance: TypeInstance,
    ) -> TypeId {
        let kind = old.kind(self.db);
        let targs = if kind.is_closure() {
            self.arguments.clone()
        } else {
            instance.type_arguments(self.db).unwrap().clone()
        };

        let new = old.clone_for_specialization(self.db);

        new.set_specialization_source(self.db, old);
        old.add_specialization(self.db, key, new);
        self.types.push(new);

        // We just copy over the type parameters as-is, as there's nothing
        // stored in them that we can't share between the different type
        // specializations.
        for param in old.type_parameters(self.db) {
            new.add_type_parameter(self.db, param);
        }

        // Within the fields and constructors of a type, referring to `Self`
        // results in the concrete type being used directly instead of a
        // placeholder. This means it doesn't really matter what type we pass
        // for `Self` as we won't use it.
        if kind.is_enum() {
            for old_cons in old.constructors(self.db) {
                let args = old_cons
                    .arguments(self.db)
                    .to_vec()
                    .into_iter()
                    .map(|v| self.specialize_with_arguments(&targs, v))
                    .collect();

                let name = old_cons.name(self.db).clone();
                let loc = old_cons.location(self.db);

                new.new_constructor(self.db, name, args, loc);
            }
        }

        for old_field in old.fields(self.db) {
            let old_typ = old_field.value_type(self.db);
            let new_typ = self.specialize_with_arguments(&targs, old_typ);
            let new_field = old_field.clone_for_specialization(self.db);

            new_field.set_value_type(self.db, new_typ);
            new.add_field(self.db, new_field);
        }

        if instance.instance_of().is_generic(self.db) || kind.is_closure() {
            new.set_type_arguments(self.db, targs);
        }

        new
    }

    fn specialize_trait_instance(
        &mut self,
        ins: TraitInstance,
    ) -> TraitInstance {
        let typ = ins.instance_of;

        if typ.specialization_source(self.db).is_some() {
            return ins;
        }

        if !ins.instance_of.is_generic(self.db) {
            typ.set_specialization_source(self.db, typ);
            return ins;
        }

        let mut args = ins.type_arguments(self.db).unwrap().clone();

        for typ in args.values_mut() {
            *typ = self.specialize(*typ);
        }

        let ins = ins
            .with_new_type_arguments(self.db, args)
            .interned(self.db, self.interned);
        let key = SpecializationKey::new(ins.type_arguments(self.db).unwrap());
        let new = typ
            .specializations(self.db)
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.specialize_trait(typ, key, ins));

        TraitInstance::new(new)
    }

    fn specialize_trait(
        &mut self,
        old: TraitId,
        key: SpecializationKey,
        instance: TraitInstance,
    ) -> TraitId {
        let new = old.clone_for_specialization(self.db);

        for param in old.type_parameters(self.db) {
            new.add_type_parameter(self.db, param);
        }

        new.set_specialization_source(self.db, old);

        if instance.instance_of().is_generic(self.db) {
            new.set_type_arguments(
                self.db,
                instance.type_arguments(self.db).unwrap().clone(),
            );
        }

        old.add_specialization(self.db, key, new);
        new
    }

    fn specialize_with_arguments(
        &mut self,
        type_arguments: &TypeArguments,
        typ: TypeRef,
    ) -> TypeRef {
        TypeSpecializer::new(
            self.db,
            self.closures,
            self.interned,
            type_arguments,
            self.surrounding_arguments,
            self.types,
            self.self_type,
        )
        .specialize(typ)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::format::format_type;
    use crate::test::{
        any, generic_instance, immutable, immutable_uni, instance, mutable,
        mutable_uni, new_closure_type, new_enum_type, new_parameter, new_trait,
        new_type, owned, parameter, placeholder, pointer, trait_instance, uni,
    };
    use crate::{Location, ModuleId, TypePlaceholder, Visibility};

    #[test]
    fn test_specialize() {
        let mut db = Database::new();
        let mut closures = Closures::new();
        let mut interned = InternedTypeArguments::new();
        let int_type = TypeId::int();
        let stype = instance(int_type);
        let par1 = new_parameter(&mut db, "A");
        let par2 = new_parameter(&mut db, "B");
        let mut targs = TypeArguments::new();
        let thing = new_type(&mut db, "Thing");
        let self_trait = TypeEnum::TraitInstance(
            trait_instance(new_trait(&mut db, "Self")).as_self_type(),
        );
        let var = TypePlaceholder::alloc(&mut db, None);

        var.assign(&mut db, owned(parameter(par1)));
        targs.assign(par1, TypeRef::int());
        targs.assign(par2, owned(instance(thing)));

        let tests = [
            // Value types as the Self type.
            (owned(self_trait), instance(int_type), TypeRef::int()),
            (mutable(self_trait), instance(int_type), TypeRef::int()),
            (immutable(self_trait), instance(int_type), TypeRef::int()),
            (uni(self_trait), instance(int_type), TypeRef::int()),
            (mutable_uni(self_trait), instance(int_type), TypeRef::int()),
            (immutable_uni(self_trait), instance(int_type), TypeRef::int()),
            // Regular types as the Self type
            (owned(self_trait), instance(thing), owned(instance(thing))),
            (mutable(self_trait), instance(thing), mutable(instance(thing))),
            (
                immutable(self_trait),
                instance(thing),
                immutable(instance(thing)),
            ),
            (uni(self_trait), instance(thing), uni(instance(thing))),
            (
                mutable_uni(self_trait),
                instance(thing),
                mutable_uni(instance(thing)),
            ),
            (
                immutable_uni(self_trait),
                instance(thing),
                immutable_uni(instance(thing)),
            ),
            // Type parameters assigned to a value type
            (owned(parameter(par1)), stype, TypeRef::int()),
            (mutable(parameter(par1)), stype, TypeRef::int()),
            (immutable(parameter(par1)), stype, TypeRef::int()),
            (uni(parameter(par1)), stype, TypeRef::int()),
            (mutable_uni(parameter(par1)), stype, TypeRef::int()),
            (immutable_uni(parameter(par1)), stype, TypeRef::int()),
            // Type parameters assigned to a regular type
            (owned(parameter(par2)), stype, owned(instance(thing))),
            (mutable(parameter(par2)), stype, mutable(instance(thing))),
            (immutable(parameter(par2)), stype, immutable(instance(thing))),
            (uni(parameter(par2)), stype, uni(instance(thing))),
            (mutable_uni(parameter(par2)), stype, mutable_uni(instance(thing))),
            (
                immutable_uni(parameter(par2)),
                stype,
                immutable_uni(instance(thing)),
            ),
            // Regular types
            (owned(instance(thing)), stype, owned(instance(thing))),
            (any(instance(thing)), stype, owned(instance(thing))),
            (mutable(instance(thing)), stype, mutable(instance(thing))),
            (immutable(instance(thing)), stype, immutable(instance(thing))),
            (uni(instance(thing)), stype, uni(instance(thing))),
            (mutable_uni(instance(thing)), stype, mutable_uni(instance(thing))),
            (
                immutable_uni(instance(thing)),
                stype,
                immutable_uni(instance(thing)),
            ),
            (pointer(instance(thing)), stype, pointer(instance(thing))),
            // Value types
            (owned(instance(int_type)), stype, owned(instance(int_type))),
            (any(instance(int_type)), stype, owned(instance(int_type))),
            (uni(instance(int_type)), stype, owned(instance(int_type))),
            (mutable(instance(int_type)), stype, owned(instance(int_type))),
            (immutable(instance(int_type)), stype, owned(instance(int_type))),
            (mutable_uni(instance(int_type)), stype, owned(instance(int_type))),
            (
                immutable_uni(instance(int_type)),
                stype,
                owned(instance(int_type)),
            ),
            // Placeholders
            (placeholder(var), stype, TypeRef::int()),
            // Other types
            (TypeRef::Never, stype, TypeRef::Never),
            (TypeRef::Error, stype, TypeRef::Error),
            (TypeRef::Unknown, stype, TypeRef::Unknown),
        ];

        for (input, stype, output) in tests {
            let mut types = Vec::new();
            let res = TypeSpecializer::new(
                &mut db,
                &mut closures,
                &mut interned,
                &targs,
                &targs,
                &mut types,
                stype,
            )
            .specialize(input);

            assert_eq!(res, output);
        }
    }

    #[test]
    fn test_specialize_regular_type() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let targs = TypeArguments::new();
        let old_ary = TypeId::array();

        old_ary.new_type_parameter(&mut db, "T".to_string());

        let old_box = new_type(&mut db, "Box");
        let int = TypeRef::int();
        let ints = owned(generic_instance(&mut db, old_ary, vec![int]));
        let field = old_box.new_field(
            &mut db,
            "value".to_string(),
            0,
            ints,
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        let old = owned(instance(old_box));
        let new = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old);

        assert_eq!(old, new);
        assert_eq!(old_box.specialization_source(&db), Some(old_box));
        assert_eq!(types.len(), 3);

        let new_ary = *old_ary.specializations(&db).values().next().unwrap();

        assert!(matches!(
            field.value_type(&db),
            TypeRef::Owned(TypeEnum::TypeInstance(i)) if i.instance_of == new_ary
        ));
    }

    #[test]
    fn test_specialize_int_and_int64() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let targs = TypeArguments::new();
        let stype = instance(TypeId::int());
        let new_int = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(TypeRef::int());

        let new_int64 = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(TypeRef::foreign_signed_int(64));

        assert_eq!(new_int, new_int64);
    }

    #[test]
    fn test_specialize_regular_type_with_constructors() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let targs = TypeArguments::new();
        let old_tid = new_enum_type(&mut db, "Option");
        let old_ary = TypeId::array();

        old_ary.new_type_parameter(&mut db, "T".to_string());

        let int = TypeRef::int();
        let ints = owned(generic_instance(&mut db, old_ary, vec![int]));

        old_tid.new_constructor(
            &mut db,
            "Some".to_string(),
            vec![ints],
            Location::default(),
        );

        let old = owned(instance(old_tid));
        let new = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old);

        assert_eq!(new, old);
        assert_eq!(format_type(&db, new), "Option");
        assert_eq!(old_tid.specialization_source(&db), Some(old_tid));

        let arg = old_tid.constructors(&db)[0].arguments(&db)[0];
        let new_ary = *old_ary.specializations(&db).values().next().unwrap();

        assert!(matches!(
            arg,
            TypeRef::Owned(TypeEnum::TypeInstance(i)) if i.instance_of == new_ary
        ));
    }

    #[test]
    fn test_specialize_generic_type() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let targs = TypeArguments::new();
        let tid = TypeId::array();
        let par = tid.new_type_parameter(&mut db, "T".to_string());
        let int = TypeRef::int();
        let string = TypeRef::string();
        let old1 = owned(generic_instance(&mut db, tid, vec![int]));
        let old2 = owned(generic_instance(&mut db, tid, vec![int]));
        let old3 = owned(generic_instance(&mut db, tid, vec![string]));

        let new1 = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old1);
        let new2 = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old1);
        let new3 = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old2);
        let new4 = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old3);

        assert_eq!(new1, new2);
        assert_eq!(new1, new3);
        assert_ne!(new1, new4);
        assert_ne!(new2, new4);
        assert_ne!(new3, new4);

        assert_eq!(tid.specializations(&db).len(), 2);

        let tids: Vec<_> = tid.specializations(&db).values().cloned().collect();

        // The Array[Int] specializations all share the same TypeId, while the
        // Array[String] specialization uses a different one.
        for (typ, idx) in [(new1, 0), (new2, 0), (new3, 0), (new4, 1)] {
            let TypeRef::Owned(TypeEnum::TypeInstance(ins)) = typ else {
                panic!("invalid type")
            };

            assert_eq!(ins.instance_of, tids[idx]);
        }

        assert_eq!(tids[0].type_parameters(&db), vec![par]);
        assert_eq!(tids[1].type_parameters(&db), vec![par]);
        assert_eq!(
            tids[0].type_arguments(&db),
            Some(&old1.type_arguments(&db))
        );
        assert_eq!(
            tids[1].type_arguments(&db),
            Some(&old3.type_arguments(&db))
        );
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn test_specialize_generic_type_with_fields() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let targs = TypeArguments::new();
        let foo_tid = new_type(&mut db, "Foo");
        let old_tid = new_type(&mut db, "Box");
        let par = old_tid.new_type_parameter(&mut db, "T".to_string());

        old_tid.new_field(
            &mut db,
            "value".to_string(),
            0,
            any(parameter(par)),
            Visibility::Public,
            ModuleId(0),
            Location::default(),
        );

        let foo = owned(instance(foo_tid));
        let old = owned(generic_instance(&mut db, old_tid, vec![foo]));
        let new = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old);

        assert_ne!(new, old);
        assert_eq!(format_type(&db, new), "Box[Foo]");

        let new_tid = *old_tid.specializations(&db).values().next().unwrap();

        assert_ne!(new_tid, old_tid);
        assert_eq!(new_tid.field(&db, "value").unwrap().value_type(&db), foo);
        assert_eq!(new_tid.specialization_source(&db), Some(old_tid));
    }

    #[test]
    fn test_specialize_generic_type_with_constructors() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let targs = TypeArguments::new();
        let foo_tid = new_type(&mut db, "Foo");
        let old_tid = new_enum_type(&mut db, "Option");
        let par = old_tid.new_type_parameter(&mut db, "T".to_string());

        old_tid.new_constructor(
            &mut db,
            "Some".to_string(),
            vec![any(parameter(par))],
            Location::default(),
        );

        let foo = owned(instance(foo_tid));
        let old = owned(generic_instance(&mut db, old_tid, vec![foo]));
        let new = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old);

        assert_ne!(new, old);
        assert_eq!(format_type(&db, new), "Option[Foo]");

        let new_tid = *old_tid.specializations(&db).values().next().unwrap();

        assert_ne!(new_tid, old_tid);
        assert_eq!(new_tid.constructors(&db)[0].arguments(&db), &[foo]);
        assert_eq!(new_tid.specialization_source(&db), Some(old_tid));
    }

    #[test]
    fn test_specialize_closure_literal() {
        let mut db = Database::new();
        let mut interned = InternedTypeArguments::new();
        let mut closures = Closures::new();
        let mut types = Vec::new();
        let stype = instance(TypeId::int());
        let par = new_parameter(&mut db, "T");
        let mut targs = TypeArguments::new();

        targs.assign(par, TypeRef::int());

        let tid = new_closure_type(&mut db, "Closure123");
        let mid = ModuleId(0);
        let loc = Location::default();

        tid.new_field(
            &mut db,
            "a".to_string(),
            0,
            owned(parameter(par)),
            Visibility::Public,
            mid,
            loc,
        );

        let old = owned(instance(tid));
        let new = TypeSpecializer::new(
            &mut db,
            &mut closures,
            &mut interned,
            &targs,
            &targs,
            &mut types,
            stype,
        )
        .specialize(old);

        assert_ne!(new, old);
        assert_eq!(new.fields(&db)[0].value_type(&db), TypeRef::int());
    }
}
