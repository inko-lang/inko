use crate::{
    ClosureId, Database, Location, Module, ModuleId, ModuleName, Trait,
    TraitId, TraitImplementation, TraitInstance, Type, TypeArguments,
    TypeBounds, TypeEnum, TypeId, TypeInstance, TypeKind, TypeParameter,
    TypeParameterId, TypePlaceholderId, TypeRef, Visibility,
};
use std::path::PathBuf;

pub(crate) fn new_module(db: &mut Database, name: &str) -> ModuleId {
    Module::alloc(db, ModuleName::new(name), PathBuf::from("foo.inko"))
}

pub(crate) fn new_type(db: &mut Database, name: &str) -> TypeId {
    Type::alloc(
        db,
        name.to_string(),
        TypeKind::Regular,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_closure_type(db: &mut Database, name: &str) -> TypeId {
    Type::alloc(
        db,
        name.to_string(),
        TypeKind::Closure,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_async_type(db: &mut Database, name: &str) -> TypeId {
    Type::alloc(
        db,
        name.to_string(),
        TypeKind::Async,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_enum_type(db: &mut Database, name: &str) -> TypeId {
    Type::alloc(
        db,
        name.to_string(),
        TypeKind::Enum,
        Visibility::Public,
        ModuleId(0),
        Location::default(),
    )
}

pub(crate) fn new_extern_type(db: &mut Database, name: &str) -> TypeId {
    Type::alloc(
        db,
        name.to_string(),
        TypeKind::Extern,
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
    type_id: TypeId,
) {
    type_id.add_trait_implementation(
        db,
        TraitImplementation { instance, bounds: TypeBounds::new() },
    );
}

pub(crate) fn owned(id: TypeEnum) -> TypeRef {
    TypeRef::Owned(id)
}

pub(crate) fn uni(id: TypeEnum) -> TypeRef {
    TypeRef::Uni(id)
}

pub(crate) fn immutable_uni(id: TypeEnum) -> TypeRef {
    TypeRef::UniRef(id)
}

pub(crate) fn mutable_uni(id: TypeEnum) -> TypeRef {
    TypeRef::UniMut(id)
}

pub(crate) fn any(id: TypeEnum) -> TypeRef {
    TypeRef::Any(id)
}

pub(crate) fn immutable(id: TypeEnum) -> TypeRef {
    TypeRef::Ref(id)
}

pub(crate) fn mutable(id: TypeEnum) -> TypeRef {
    TypeRef::Mut(id)
}

pub(crate) fn placeholder(id: TypePlaceholderId) -> TypeRef {
    TypeRef::Placeholder(id)
}

pub(crate) fn pointer(id: TypeEnum) -> TypeRef {
    TypeRef::Pointer(id)
}

pub(crate) fn instance(type_id: TypeId) -> TypeEnum {
    TypeEnum::TypeInstance(TypeInstance::new(type_id))
}

pub(crate) fn parameter(id: TypeParameterId) -> TypeEnum {
    TypeEnum::TypeParameter(id)
}

pub(crate) fn rigid(id: TypeParameterId) -> TypeEnum {
    TypeEnum::RigidTypeParameter(id)
}

pub(crate) fn closure(id: ClosureId) -> TypeEnum {
    TypeEnum::Closure(id)
}

pub(crate) fn generic_instance(
    db: &mut Database,
    type_id: TypeId,
    arguments: Vec<TypeRef>,
) -> TypeEnum {
    let mut args = TypeArguments::new();

    for (param, arg) in
        type_id.type_parameters(db).into_iter().zip(arguments.into_iter())
    {
        args.assign(param, arg);
    }

    TypeEnum::TypeInstance(TypeInstance::generic(db, type_id, args))
}

pub(crate) fn generic_trait_instance_id(
    db: &mut Database,
    trait_id: TraitId,
    arguments: Vec<TypeRef>,
) -> TypeEnum {
    TypeEnum::TraitInstance(generic_trait_instance(db, trait_id, arguments))
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

pub(crate) fn trait_instance_id(trait_id: TraitId) -> TypeEnum {
    TypeEnum::TraitInstance(trait_instance(trait_id))
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
