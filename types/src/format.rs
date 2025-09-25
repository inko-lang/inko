//! Formatting of types.
use crate::{
    Arguments, ClosureId, ClosureKind, Database, ForeignType, Inline, MethodId,
    MethodKind, ModuleId, Ownership, Sign, TraitId, TraitInstance,
    TypeArguments, TypeEnum, TypeId, TypeInstance, TypeKind, TypeParameterId,
    TypePlaceholderId, TypeRef, Visibility, NEVER_TYPE, SELF_TYPE,
};

const MAX_FORMATTING_DEPTH: usize = 8;

pub fn format_type<T: FormatType>(db: &Database, typ: T) -> String {
    TypeFormatter::new(db, None, None).format(typ)
}

pub fn format_type_with_arguments<T: FormatType>(
    db: &Database,
    arguments: &TypeArguments,
    typ: T,
) -> String {
    TypeFormatter::new(db, None, Some(arguments)).format(typ)
}

pub fn type_parameter_capabilities(
    db: &Database,
    id: TypeParameterId,
) -> Option<&'static str> {
    let param = id.get(db);

    if param.copy {
        Some("copy")
    } else if param.mutable {
        Some("mut")
    } else {
        None
    }
}

fn format_type_parameter_without_argument(
    id: TypeParameterId,
    buffer: &mut TypeFormatter,
    owned: bool,
    requirements: bool,
) {
    let param = id.get(buffer.db);

    if owned {
        buffer.write_ownership("move ");
    }

    buffer.write(&param.name);

    let capa = if let Some(v) = type_parameter_capabilities(buffer.db, id) {
        buffer.write(": ");
        buffer.write(v);
        true
    } else {
        false
    };

    if requirements && id.has_requirements(buffer.db) {
        if capa {
            buffer.write(" + ");
        } else {
            buffer.write(": ");
        }

        for (idx, req) in id.requirements(buffer.db).into_iter().enumerate() {
            if idx > 0 {
                buffer.write(" + ");
            }

            req.format_type(buffer);
        }
    }
}

fn format_type_parameter(
    param: TypeParameterId,
    buffer: &mut TypeFormatter,
    owned: bool,
) {
    // Formatting type parameters is a bit tricky, as they may be assigned
    // to themselves directly or through a placeholder. The below code isn't
    // going to win any awards, but it should ensure we don't blow the stack
    // when trying to format recursive type parameters, such as
    // `T -> placeholder -> T`.
    if let Some(arg) = buffer.type_arguments.and_then(|a| a.get(param)) {
        if let TypeRef::Placeholder(p) = arg {
            match p.value(buffer.db) {
                Some(t) if t.as_type_parameter(buffer.db) == Some(param) => {
                    format_type_parameter_without_argument(
                        param, buffer, owned, false,
                    )
                }
                Some(t) => if owned { t.as_owned(buffer.db) } else { t }
                    .format_type(buffer),
                None => format_type_parameter_without_argument(
                    param, buffer, owned, false,
                ),
            }

            return;
        }

        if arg.as_type_parameter(buffer.db) == Some(param) {
            format_type_parameter_without_argument(param, buffer, owned, false);
            return;
        }

        if owned { arg.as_owned(buffer.db) } else { arg }.format_type(buffer);
    } else {
        format_type_parameter_without_argument(param, buffer, owned, false);
    };
}

/// A buffer for formatting type names.
///
/// We use a simple wrapper around a String so we can more easily change the
/// implementation in the future if necessary.
pub struct TypeFormatter<'a> {
    db: &'a Database,
    type_arguments: Option<&'a TypeArguments>,
    self_type: Option<TypeEnum>,
    buffer: String,
    depth: usize,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(
        db: &'a Database,
        self_type: Option<TypeEnum>,
        type_arguments: Option<&'a TypeArguments>,
    ) -> Self {
        Self { db, self_type, type_arguments, buffer: String::new(), depth: 0 }
    }

    pub fn with_self_type(
        db: &'a Database,
        self_type: TypeEnum,
        type_arguments: Option<&'a TypeArguments>,
    ) -> Self {
        TypeFormatter::new(db, Some(self_type), type_arguments)
    }

    pub fn format<T: FormatType>(mut self, typ: T) -> String {
        typ.format_type(&mut self);
        self.buffer
    }

    pub(crate) fn descend<F: FnOnce(&mut TypeFormatter)>(&mut self, block: F) {
        if self.depth == MAX_FORMATTING_DEPTH {
            self.write("...");
        } else {
            self.depth += 1;

            block(self);

            self.depth -= 1;
        }
    }

    pub(crate) fn write(&mut self, thing: &str) {
        self.buffer.push_str(thing);
    }

    /// If a uni/ref/mut value wraps a type parameter, and that parameter is
    /// assigned another value with ownership, you can end up with e.g. `ref mut
    /// T` or `uni uni T`. This method provides a simple way of preventing this
    /// from happening, without complicating the type formatting process.
    pub(crate) fn write_ownership(&mut self, thing: &str) {
        if !self.buffer.ends_with(thing) {
            self.write(thing);
        }
    }

    pub(crate) fn type_parameters(&mut self, parameters: &[TypeParameterId]) {
        if parameters.is_empty() {
            return;
        }

        self.write("[");

        for (index, &param) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            format_type_parameter_without_argument(param, self, false, true);
        }

        self.write("]");
    }

    pub(crate) fn type_arguments(
        &mut self,
        parameters: &[TypeParameterId],
        arguments: Option<&TypeArguments>,
    ) {
        for (index, &param) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            match arguments.and_then(|a| a.get(param)) {
                Some(TypeRef::Placeholder(id))
                    if id.value(self.db).is_none() =>
                {
                    id.format_type(self);
                }
                Some(typ) => typ.format_type(self),
                _ => param.format_type(self),
            }
        }
    }

    pub(crate) fn arguments(
        &mut self,
        arguments: &Arguments,
        include_name: bool,
    ) {
        if arguments.len() == 0 {
            return;
        }

        self.write("(");

        for (index, arg) in arguments.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            if include_name {
                self.write(&arg.name);
                self.write(": ");
            }

            arg.value_type.format_type(self);
        }

        self.write(")");
    }

    pub(crate) fn return_type(&mut self, typ: TypeRef) {
        match typ {
            TypeRef::Placeholder(id) if id.value(self.db).is_none() => {}
            TypeRef::Unknown => {}
            _ if typ == TypeRef::nil() => {}
            _ => {
                self.write(" -> ");
                typ.format_type(self);
            }
        }
    }
}

/// A type of which the name can be formatted into something human-readable.
pub trait FormatType {
    fn format_type(&self, buffer: &mut TypeFormatter);
}

impl FormatType for TypePlaceholderId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        if let Some(value) = self.value(buffer.db) {
            value.format_type(buffer);
            return;
        }

        let ownership = match self.ownership {
            Ownership::Any => "",
            Ownership::Owned => "move ",
            Ownership::Uni => "uni ",
            Ownership::Ref => "ref ",
            Ownership::Mut => "mut ",
            Ownership::UniMut => "uni mut ",
            Ownership::UniRef => "uni ref ",
            Ownership::Pointer => {
                buffer.write("Pointer[");

                if let Some(req) = self.required(buffer.db) {
                    req.format_type(buffer);
                } else {
                    buffer.write("?");
                }

                buffer.write("]");
                return;
            }
        };

        if !ownership.is_empty() {
            buffer.write_ownership(ownership);
        }

        if let Some(req) = self.required(buffer.db) {
            req.format_type(buffer);
        } else {
            buffer.write("?");
        }
    }
}

impl FormatType for TypeParameterId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        format_type_parameter(*self, buffer, false);
    }
}

impl FormatType for TraitId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
        buffer.type_parameters(&self.type_parameters(buffer.db));
    }
}

impl FormatType for TraitInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        if self.self_type {
            match buffer.self_type {
                Some(TypeEnum::TraitInstance(_)) | None => {
                    buffer.write(SELF_TYPE);
                    return;
                }
                Some(e) => return e.format_type(buffer),
            }
        }

        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            buffer.write(&ins_of.name);

            if !ins_of.type_parameters.is_empty() {
                let params: Vec<_> =
                    ins_of.type_parameters.values().cloned().collect();

                buffer.write("[");
                buffer.type_arguments(&params, self.type_arguments(buffer.db));
                buffer.write("]");
            }
        });
    }
}

impl FormatType for TypeId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
        buffer.type_parameters(&self.type_parameters(buffer.db));
    }
}

impl FormatType for TypeInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            if !matches!(ins_of.kind, TypeKind::Tuple) {
                buffer.write(&ins_of.name);
            }

            if !ins_of.type_parameters.is_empty() {
                let (open, close) = if let TypeKind::Tuple = ins_of.kind {
                    ("(", ")")
                } else {
                    ("[", "]")
                };

                let params: Vec<_> =
                    ins_of.type_parameters.values().cloned().collect();

                buffer.write(open);
                buffer.type_arguments(&params, self.type_arguments(buffer.db));
                buffer.write(close);
            }
        });
    }
}

impl FormatType for MethodId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        let block = self.get(buffer.db);

        buffer.write("fn ");

        if block.visibility == Visibility::Public {
            buffer.write("pub ");
        }

        if let Inline::Always = block.inline {
            buffer.write("inline ");
        }

        match block.kind {
            MethodKind::Async => buffer.write("async "),
            MethodKind::AsyncMutable => buffer.write("async mut "),
            MethodKind::Static | MethodKind::Constructor => {
                buffer.write("static ")
            }
            MethodKind::Moving => buffer.write("move "),
            MethodKind::Mutable | MethodKind::Destructor => {
                buffer.write("mut ")
            }
            MethodKind::Extern => buffer.write("extern "),
            MethodKind::Instance => {}
        }

        let params: Vec<_> = block.type_parameters.values().cloned().collect();

        buffer.write(&block.name);
        buffer.type_parameters(&params);
        buffer.arguments(&block.arguments, true);
        buffer.return_type(block.return_type);
    }
}

impl FormatType for ModuleId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name.to_string());
    }
}

impl FormatType for ClosureId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let fun = self.get(buffer.db);

            if fun.capture_by_moving
                || matches!(fun.kind.get(), ClosureKind::Moving)
            {
                buffer.write("fn move");
            } else {
                buffer.write("fn");
            }

            if fun.arguments.len() > 0 {
                buffer.write(" ");
            }

            buffer.arguments(&fun.arguments, false);
            buffer.return_type(fun.return_type);
        });
    }
}

impl FormatType for TypeRef {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeRef::Owned(TypeEnum::TypeParameter(id)) => {
                format_type_parameter(*id, buffer, true);
            }
            TypeRef::Owned(TypeEnum::RigidTypeParameter(id)) => {
                format_type_parameter_without_argument(
                    *id, buffer, true, false,
                );
            }
            TypeRef::Owned(id) => id.format_type(buffer),
            TypeRef::Any(id) => id.format_type(buffer),
            TypeRef::Uni(id) => {
                if !self.is_value_type(buffer.db) {
                    buffer.write_ownership("uni ");
                }

                id.format_type(buffer);
            }
            TypeRef::UniRef(id) => {
                if !self.is_value_type(buffer.db) {
                    buffer.write_ownership("uni ref ");
                }

                id.format_type(buffer);
            }
            TypeRef::UniMut(id) => {
                if !self.is_value_type(buffer.db) {
                    buffer.write_ownership("uni mut ");
                }

                id.format_type(buffer);
            }
            TypeRef::Ref(id) => {
                if !self.is_value_type(buffer.db) {
                    buffer.write_ownership("ref ");
                }

                id.format_type(buffer);
            }
            TypeRef::Mut(id) => {
                if !self.is_value_type(buffer.db) {
                    buffer.write_ownership("mut ");
                }

                id.format_type(buffer);
            }
            TypeRef::Never => buffer.write(NEVER_TYPE),
            TypeRef::Error => buffer.write("<error>"),
            TypeRef::Unknown => buffer.write("<unknown>"),
            TypeRef::Placeholder(id) => id.format_type(buffer),
            TypeRef::Pointer(typ) => {
                buffer.write("Pointer[");
                typ.format_type(buffer);
                buffer.write("]");
            }
        };
    }
}

impl FormatType for TypeEnum {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeEnum::Type(id) => id.format_type(buffer),
            TypeEnum::Trait(id) => id.format_type(buffer),
            TypeEnum::Module(id) => id.format_type(buffer),
            TypeEnum::TypeInstance(ins) => ins.format_type(buffer),
            TypeEnum::TraitInstance(id) => id.format_type(buffer),
            TypeEnum::TypeParameter(id) => id.format_type(buffer),
            TypeEnum::RigidTypeParameter(id)
            | TypeEnum::AtomicTypeParameter(id) => {
                format_type_parameter_without_argument(
                    *id, buffer, false, false,
                );
            }
            TypeEnum::Closure(id) => id.format_type(buffer),
            TypeEnum::Foreign(ForeignType::Int(size, Sign::Signed)) => {
                buffer.write(&format!("Int{}", size))
            }
            TypeEnum::Foreign(ForeignType::Int(size, Sign::Unsigned)) => {
                buffer.write(&format!("UInt{}", size))
            }
            TypeEnum::Foreign(ForeignType::Float(size)) => {
                buffer.write(&format!("Float{}", size))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{
        any, immutable, immutable_uni, instance, mutable, mutable_uni,
        new_parameter, new_type, owned, placeholder, uni,
    };
    use crate::{
        Block, Closure, Database, Inline, Location, Method, MethodKind, Module,
        ModuleId, ModuleName, Trait, TraitInstance, Type, TypeArguments,
        TypeEnum, TypeInstance, TypeKind, TypeParameter, TypePlaceholder,
        TypeRef, Visibility,
    };

    #[test]
    fn test_trait_instance_format_type_with_regular_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "A".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let trait_ins = TraitInstance::new(trait_id);

        assert_eq!(format_type(&db, trait_ins), "A".to_string());
    }

    #[test]
    fn test_trait_instance_format_type_with_self_type() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "Equal".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let trait_ins = TraitInstance::new(trait_id).as_self_type();
        let stype = instance(TypeId::int());
        let fmt1 = TypeFormatter::with_self_type(&db, stype, None);
        let fmt2 = TypeFormatter::new(&db, None, None);

        assert_eq!(fmt1.format(trait_ins), "Int".to_string());
        assert_eq!(fmt2.format(trait_ins), "Self".to_string());
    }

    #[test]
    fn test_trait_instance_format_type_with_generic_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param1 = trait_id.new_type_parameter(&mut db, "A".to_string());

        trait_id.new_type_parameter(&mut db, "B".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::int());

        let trait_ins = TraitInstance::generic(&mut db, trait_id, targs);

        assert_eq!(format_type(&db, trait_ins), "ToString[Int, B]");
    }

    #[test]
    fn test_method_id_format_type_with_instance_method() {
        let mut db = Database::new();
        let type_a = Type::alloc(
            &mut db,
            "A".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let type_b = Type::alloc(
            &mut db,
            "B".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let type_d = Type::alloc(
            &mut db,
            "D".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        let ins_a =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_a)));

        let ins_b =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_b)));

        let ins_d =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_d)));

        let loc = Location::default();

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a, loc);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b, loc);
        block.set_return_type(&mut db, ins_d);

        assert_eq!(format_type(&db, block), "fn foo(a: A, b: B) -> D");
    }

    #[test]
    fn test_method_id_format_type_with_moving_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Moving,
        );

        block.set_return_type(&mut db, TypeRef::int());

        assert_eq!(format_type(&db, block), "fn move foo -> Int");
    }

    #[test]
    fn test_method_id_format_type_with_type_parameters() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        let param1 = block.new_type_parameter(&mut db, "A".to_string());

        param1.set_mutable(&mut db);
        block.new_type_parameter(&mut db, "B".to_string());
        block.set_return_type(&mut db, TypeRef::int());

        assert_eq!(format_type(&db, block), "fn static foo[A: mut, B] -> Int");
    }

    #[test]
    fn test_method_id_format_type_with_static_method() {
        let mut db = Database::new();
        let loc = Location::default();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::int(),
            TypeRef::int(),
            loc,
        );
        block.set_return_type(&mut db, TypeRef::int());

        assert_eq!(format_type(&db, block), "fn static foo(a: Int) -> Int");
    }

    #[test]
    fn test_method_id_format_type_with_async_method() {
        let mut db = Database::new();
        let loc = Location::default();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Async,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::int(),
            TypeRef::int(),
            loc,
        );
        block.set_return_type(&mut db, TypeRef::int());

        assert_eq!(format_type(&db, block), "fn async foo(a: Int) -> Int");
    }

    #[test]
    fn test_method_id_format_type_with_inline_method() {
        let mut db = Database::new();
        let loc = Location::default();
        let method = Method::alloc(
            &mut db,
            ModuleId(0),
            Location::default(),
            "foo".to_string(),
            Visibility::Public,
            MethodKind::Mutable,
        );

        method.set_inline(&mut db, Inline::Always);
        method.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::int(),
            TypeRef::int(),
            loc,
        );
        method.set_return_type(&mut db, TypeRef::int());

        assert_eq!(
            format_type(&db, method),
            "fn pub inline mut foo(a: Int) -> Int"
        );
    }

    #[test]
    fn test_closure_id_format_type_never_returns() {
        let mut db = Database::new();
        let block = Closure::alloc(&mut db, false);

        block.set_return_type(&mut db, TypeRef::Never);

        assert_eq!(format_type(&db, block), "fn -> Never");
    }

    #[test]
    fn test_type_id_format_type_with_type() {
        let mut db = Database::new();
        let id = TypeEnum::Type(Type::alloc(
            &mut db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        ));

        assert_eq!(format_type(&db, id), "String");
    }

    #[test]
    fn test_type_id_format_type_with_generic_type() {
        let mut db = Database::new();
        let to_a = Trait::alloc(
            &mut db,
            "ToA".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let to_b = Trait::alloc(
            &mut db,
            "ToB".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let id = Type::alloc(
            &mut db,
            "Foo".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        let param1 = id.new_type_parameter(&mut db, "A".to_string());

        id.new_type_parameter(&mut db, "B".to_string());
        param1.add_requirements(&mut db, vec![TraitInstance::new(to_a)]);
        param1.add_requirements(&mut db, vec![TraitInstance::new(to_b)]);
        param1.set_mutable(&mut db);

        assert_eq!(
            format_type(&db, TypeEnum::Type(id)),
            "Foo[A: mut + ToA + ToB, B]"
        );
    }

    #[test]
    fn test_type_id_format_type_with_trait() {
        let mut db = Database::new();
        let id = TypeEnum::Trait(Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        ));

        assert_eq!(format_type(&db, id), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_generic_trait() {
        let mut db = Database::new();
        let to_a = Trait::alloc(
            &mut db,
            "ToA".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let to_b = Trait::alloc(
            &mut db,
            "ToB".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let id = Trait::alloc(
            &mut db,
            "Foo".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        let param1 = id.new_type_parameter(&mut db, "A".to_string());

        id.new_type_parameter(&mut db, "B".to_string());
        param1.add_requirements(&mut db, vec![TraitInstance::new(to_a)]);
        param1.add_requirements(&mut db, vec![TraitInstance::new(to_b)]);
        param1.set_mutable(&mut db);

        assert_eq!(
            format_type(&db, TypeEnum::Trait(id)),
            "Foo[A: mut + ToA + ToB, B]"
        );
    }

    #[test]
    fn test_type_id_format_type_with_module() {
        let mut db = Database::new();
        let id = TypeEnum::Module(Module::alloc(
            &mut db,
            ModuleName::new("foo::bar"),
            "foo/bar.inko".into(),
        ));

        assert_eq!(format_type(&db, id), "foo::bar");
    }

    #[test]
    fn test_type_id_format_type_with_type_instance() {
        let mut db = Database::new();
        let id = Type::alloc(
            &mut db,
            "String".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let ins = TypeEnum::TypeInstance(TypeInstance::new(id));

        assert_eq!(format_type(&db, ins), "String");
    }

    #[test]
    fn test_type_id_format_type_with_generic_type_instance_without_arguments() {
        let mut db = Database::new();
        let id = Type::alloc(
            &mut db,
            "Array".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );

        id.new_type_parameter(&mut db, "T".to_string());

        let ins = TypeEnum::TypeInstance(TypeInstance::new(id));

        assert_eq!(format_type(&db, ins), "Array[T]");
    }

    #[test]
    fn test_type_id_format_type_with_tuple_instance() {
        let mut db = Database::new();
        let id = Type::alloc(
            &mut db,
            "MyTuple".to_string(),
            TypeKind::Tuple,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param1 = id.new_type_parameter(&mut db, "A".to_string());
        let param2 = id.new_type_parameter(&mut db, "B".to_string());
        let mut args = TypeArguments::new();

        args.assign(param1, TypeRef::int());
        args.assign(param2, TypeRef::Never);

        let ins =
            TypeEnum::TypeInstance(TypeInstance::generic(&mut db, id, args));

        assert_eq!(format_type(&db, ins), "(Int, Never)");
    }

    #[test]
    fn test_type_id_format_type_with_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let ins = TypeEnum::TraitInstance(TraitInstance::new(id));

        assert_eq!(format_type(&db, ins), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_generic_type_instance() {
        let mut db = Database::new();
        let id = Type::alloc(
            &mut db,
            "Thing".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::int());

        let ins =
            TypeEnum::TypeInstance(TypeInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "Thing[Int, E]");
    }

    #[test]
    fn test_type_id_format_type_with_generic_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToFoo".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::int());

        let ins =
            TypeEnum::TraitInstance(TraitInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "ToFoo[Int, E]");
    }

    #[test]
    fn test_type_id_format_type_with_type_parameter() {
        let mut db = Database::new();
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param_ins = TypeEnum::TypeParameter(param);
        let to_string_ins = TraitInstance::new(to_string);

        param.add_requirements(&mut db, vec![to_string_ins]);

        assert_eq!(format_type(&db, param_ins), "T");
        assert_eq!(format_type(&db, TypeRef::Owned(param_ins)), "move T");
    }

    #[test]
    fn test_type_id_format_type_with_rigid_type_parameter() {
        let mut db = Database::new();
        let param = TypeParameter::alloc(&mut db, "T".to_string());
        let to_string = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let param_ins = TypeEnum::RigidTypeParameter(param);
        let to_string_ins = TraitInstance::new(to_string);

        param.add_requirements(&mut db, vec![to_string_ins]);

        assert_eq!(format_type(&db, param_ins), "T");
    }

    #[test]
    fn test_type_id_format_type_with_closure() {
        let mut db = Database::new();
        let loc = Location::default();
        let type_a = Type::alloc(
            &mut db,
            "A".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let type_b = Type::alloc(
            &mut db,
            "B".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let type_d = Type::alloc(
            &mut db,
            "D".to_string(),
            TypeKind::Regular,
            Visibility::Private,
            ModuleId(0),
            Location::default(),
        );
        let block = Closure::alloc(&mut db, true);

        let ins_a =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_a)));

        let ins_b =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_b)));

        let ins_d =
            TypeRef::Owned(TypeEnum::TypeInstance(TypeInstance::new(type_d)));

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a, loc);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b, loc);
        block.set_return_type(&mut db, ins_d);

        let block_ins = TypeEnum::Closure(block);

        assert_eq!(format_type(&db, block_ins), "fn move (A, B) -> D");
    }

    #[test]
    fn test_type_ref_type_name() {
        let mut db = Database::new();
        let cls = new_type(&mut db, "A");
        let ins = instance(cls);
        let int = instance(TypeId::int());
        let param = TypeEnum::TypeParameter(TypeParameter::alloc(
            &mut db,
            "T".to_string(),
        ));
        let var1 = TypePlaceholder::alloc(&mut db, None);
        let var2 = TypePlaceholder::alloc(&mut db, None);

        var1.assign(&mut db, owned(ins));
        var2.assign(&mut db, owned(int));

        // Regular types
        assert_eq!(format_type(&db, owned(ins)), "A".to_string());
        assert_eq!(format_type(&db, uni(ins)), "uni A".to_string());
        assert_eq!(format_type(&db, mutable_uni(ins)), "uni mut A".to_string());
        assert_eq!(
            format_type(&db, immutable_uni(ins)),
            "uni ref A".to_string()
        );
        assert_eq!(format_type(&db, any(param)), "T".to_string());
        assert_eq!(format_type(&db, immutable(ins)), "ref A".to_string());
        assert_eq!(format_type(&db, mutable(ins)), "mut A".to_string());

        // Value types
        assert_eq!(format_type(&db, owned(int)), "Int".to_string());
        assert_eq!(format_type(&db, uni(int)), "Int".to_string());
        assert_eq!(format_type(&db, mutable_uni(int)), "Int".to_string());
        assert_eq!(format_type(&db, immutable_uni(int)), "Int".to_string());
        assert_eq!(format_type(&db, immutable(int)), "Int".to_string());
        assert_eq!(format_type(&db, mutable(int)), "Int".to_string());

        assert_eq!(format_type(&db, TypeRef::Never), "Never".to_string());
        assert_eq!(format_type(&db, TypeRef::Error), "<error>".to_string());
        assert_eq!(format_type(&db, TypeRef::Unknown), "<unknown>".to_string());
    }

    #[test]
    fn test_ctype_format() {
        let db = Database::new();

        assert_eq!(format_type(&db, TypeRef::foreign_signed_int(8)), "Int8");
        assert_eq!(format_type(&db, TypeRef::foreign_signed_int(16)), "Int16");
        assert_eq!(format_type(&db, TypeRef::foreign_signed_int(32)), "Int32");
        assert_eq!(format_type(&db, TypeRef::foreign_signed_int(64)), "Int64");
        assert_eq!(format_type(&db, TypeRef::foreign_unsigned_int(8)), "UInt8");
        assert_eq!(
            format_type(&db, TypeRef::foreign_unsigned_int(16)),
            "UInt16"
        );
        assert_eq!(
            format_type(&db, TypeRef::foreign_unsigned_int(32)),
            "UInt32"
        );
        assert_eq!(
            format_type(&db, TypeRef::foreign_unsigned_int(64)),
            "UInt64"
        );
        assert_eq!(
            format_type(
                &db,
                TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                    8,
                    Sign::Signed
                )))
            ),
            "Pointer[Int8]"
        );
        assert_eq!(
            format_type(
                &db,
                TypeRef::pointer(TypeEnum::Foreign(ForeignType::Int(
                    8,
                    Sign::Unsigned
                )))
            ),
            "Pointer[UInt8]"
        );
    }

    #[test]
    fn test_format_placeholder_with_ownership() {
        let mut db = Database::new();
        let param = new_parameter(&mut db, "T");
        let mut p1 = TypePlaceholder::alloc(&mut db, Some(param));
        let tests = vec![
            (Ownership::Any, "T"),
            (Ownership::Owned, "move T"),
            (Ownership::Uni, "uni T"),
            (Ownership::Ref, "ref T"),
            (Ownership::Mut, "mut T"),
            (Ownership::UniRef, "uni ref T"),
            (Ownership::UniMut, "uni mut T"),
            (Ownership::Pointer, "Pointer[T]"),
        ];

        for (ownership, format) in tests {
            p1.ownership = ownership;
            assert_eq!(format_type(&db, placeholder(p1)), format);
        }
    }

    #[test]
    fn test_format_placeholder_with_assigned_value() {
        let mut db = Database::new();
        let heap = owned(instance(new_type(&mut db, "Heap")));
        let stack = owned(instance(TypeId::int()));
        let mut var = TypePlaceholder::alloc(&mut db, None);
        let tests = vec![
            (heap, Ownership::Any, "Heap"),
            (heap, Ownership::Owned, "Heap"),
            (heap, Ownership::Uni, "uni Heap"),
            (heap, Ownership::Ref, "ref Heap"),
            (heap, Ownership::Mut, "mut Heap"),
            (heap, Ownership::UniRef, "uni ref Heap"),
            (heap, Ownership::UniMut, "uni mut Heap"),
            (heap, Ownership::Pointer, "Pointer[Heap]"),
            (stack, Ownership::Any, "Int"),
            (stack, Ownership::Owned, "Int"),
            (stack, Ownership::Uni, "Int"),
            (stack, Ownership::Ref, "Int"),
            (stack, Ownership::Mut, "Int"),
            (stack, Ownership::UniRef, "Int"),
            (stack, Ownership::UniMut, "Int"),
            (stack, Ownership::Pointer, "Pointer[Int]"),
        ];

        for (typ, ownership, format) in tests {
            var.ownership = ownership;
            var.assign(&mut db, typ);
            assert_eq!(format_type(&db, placeholder(var)), format);
        }
    }

    #[test]
    fn test_format_placeholder_with_ownership_without_requirement() {
        let mut db = Database::new();
        let mut p1 = TypePlaceholder::alloc(&mut db, None);
        let tests = vec![
            (Ownership::Any, "?"),
            (Ownership::Owned, "move ?"),
            (Ownership::Uni, "uni ?"),
            (Ownership::Ref, "ref ?"),
            (Ownership::Mut, "mut ?"),
            (Ownership::UniRef, "uni ref ?"),
            (Ownership::UniMut, "uni mut ?"),
        ];

        for (ownership, format) in tests {
            p1.ownership = ownership;
            assert_eq!(format_type(&db, placeholder(p1)), format);
        }
    }
}
