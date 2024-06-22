use crate::{
    Class, ClassId, ClassInstance, ClassKind, ClosureId, Database, Location,
    Module, ModuleId, ModuleName, Trait, TraitId, TraitImplementation,
    TraitInstance, TypeArguments, TypeBounds, TypeId, TypeParameter,
    TypeParameterId, TypePlaceholderId, TypeRef, Visibility,
};
use std::path::PathBuf;

pub(crate) fn new_module(db: &mut Database, name: &str) -> ModuleId {
    Module::alloc(db, ModuleName::new(name), PathBuf::from("foo.inko"))
}

pub(crate) fn new_class(db: &mut Database, name: &str) -> ClassId {
    Class::alloc(
        db,
        name.to_string(),
        ClassKind::Regular,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_async_class(db: &mut Database, name: &str) -> ClassId {
    Class::alloc(
        db,
        name.to_string(),
        ClassKind::Async,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_enum_class(db: &mut Database, name: &str) -> ClassId {
    Class::alloc(
        db,
        name.to_string(),
        ClassKind::Enum,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_extern_class(db: &mut Database, name: &str) -> ClassId {
    Class::alloc(
        db,
        name.to_string(),
        ClassKind::Extern,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_trait(db: &mut Database, name: &str) -> TraitId {
    Trait::alloc(
        db,
        name.to_string(),
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_parameter(db: &mut Database, name: &str) -> TypeParameterId {
    TypeParameter::alloc(db, name.to_string())
}

pub(crate) fn implement(
    db: &mut Database,
    instance: TraitInstance,
    class: ClassId,
) {
    class.add_trait_implementation(
        db,
        TraitImplementation { instance, bounds: TypeBounds::new() },
    );
}

pub(crate) fn owned(id: TypeId) -> TypeRef {
    TypeRef::Owned(id)
}

pub(crate) fn uni(id: TypeId) -> TypeRef {
    TypeRef::Uni(id)
}

pub(crate) fn immutable_uni(id: TypeId) -> TypeRef {
    TypeRef::UniRef(id)
}

pub(crate) fn mutable_uni(id: TypeId) -> TypeRef {
    TypeRef::UniMut(id)
}

pub(crate) fn any(id: TypeId) -> TypeRef {
    TypeRef::Any(id)
}

pub(crate) fn immutable(id: TypeId) -> TypeRef {
    TypeRef::Ref(id)
}

pub(crate) fn mutable(id: TypeId) -> TypeRef {
    TypeRef::Mut(id)
}

pub(crate) fn placeholder(id: TypePlaceholderId) -> TypeRef {
    TypeRef::Placeholder(id)
}

pub(crate) fn pointer(id: TypeId) -> TypeRef {
    TypeRef::Pointer(id)
}

pub(crate) fn instance(class: ClassId) -> TypeId {
    TypeId::ClassInstance(ClassInstance::new(class))
}

pub(crate) fn parameter(id: TypeParameterId) -> TypeId {
    TypeId::TypeParameter(id)
}

pub(crate) fn rigid(id: TypeParameterId) -> TypeId {
    TypeId::RigidTypeParameter(id)
}

pub(crate) fn closure(id: ClosureId) -> TypeId {
    TypeId::Closure(id)
}

pub(crate) fn generic_instance_id(
    db: &mut Database,
    class: ClassId,
    arguments: Vec<TypeRef>,
) -> TypeId {
    let mut args = TypeArguments::new();

    for (param, arg) in
        class.type_parameters(db).into_iter().zip(arguments.into_iter())
    {
        args.assign(param, arg);
    }

    TypeId::ClassInstance(ClassInstance::generic(db, class, args))
}

pub(crate) fn generic_trait_instance_id(
    db: &mut Database,
    trait_id: TraitId,
    arguments: Vec<TypeRef>,
) -> TypeId {
    TypeId::TraitInstance(generic_trait_instance(db, trait_id, arguments))
}

pub(crate) fn generic_trait_instance(
    db: &mut Database,
    trait_id: TraitId,
    arguments: Vec<TypeRef>,
) -> TraitInstance {
    let mut args = TypeArguments::new();

    for (param, arg) in
        trait_id.type_parameters(db).into_iter().zip(arguments.into_iter())
    {
        args.assign(param, arg);
    }

    TraitInstance::generic(db, trait_id, args)
}

pub(crate) fn trait_instance(trait_id: TraitId) -> TraitInstance {
    TraitInstance::new(trait_id)
}

pub(crate) fn trait_instance_id(trait_id: TraitId) -> TypeId {
    TypeId::TraitInstance(trait_instance(trait_id))
}

pub(crate) fn type_arguments(
    pairs: Vec<(TypeParameterId, TypeRef)>,
) -> TypeArguments {
    let mut args = TypeArguments::new();

    for (param, typ) in pairs {
        args.assign(param, typ);
    }

    args
}

pub(crate) fn type_bounds(
    pairs: Vec<(TypeParameterId, TypeParameterId)>,
) -> TypeBounds {
    let mut bounds = TypeBounds::new();

    for (param, bound) in pairs {
        bounds.set(param, bound);
    }

    bounds
}
