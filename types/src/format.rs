//! Formatting of types.
use crate::{
    Arguments, ClassId, ClassInstance, ClassKind, ClosureId, Database,
    ForeignType, MethodId, MethodKind, ModuleId, TraitId, TraitInstance,
    TypeArguments, TypeId, TypeParameterId, TypePlaceholderId, TypeRef,
    Visibility,
};

const MAX_FORMATTING_DEPTH: usize = 8;

pub fn format_type<T: FormatType>(db: &Database, typ: T) -> String {
    TypeFormatter::new(db, None).format(typ)
}

pub fn format_type_with_arguments<T: FormatType>(
    db: &Database,
    arguments: &TypeArguments,
    typ: T,
) -> String {
    TypeFormatter::new(db, Some(arguments)).format(typ)
}

/// A buffer for formatting type names.
///
/// We use a simple wrapper around a String so we can more easily change the
/// implementation in the future if necessary.
pub struct TypeFormatter<'a> {
    db: &'a Database,
    type_arguments: Option<&'a TypeArguments>,
    buffer: String,
    depth: usize,
}

impl<'a> TypeFormatter<'a> {
    pub fn new(
        db: &'a Database,
        type_arguments: Option<&'a TypeArguments>,
    ) -> Self {
        Self { db, type_arguments, buffer: String::new(), depth: 0 }
    }

    pub fn verbose(
        db: &'a Database,
        type_arguments: Option<&'a TypeArguments>,
    ) -> Self {
        Self { db, type_arguments, buffer: String::new(), depth: 0 }
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
    /// assigned another value with ownership, you can end up with e.g.
    /// `ref mut T` or `uni uni T`. This method provides a simple way of
    /// preventing this from happening, without complicating the type formatting
    /// process.
    pub(crate) fn write_ownership(&mut self, thing: &str) {
        if !self.buffer.ends_with(thing) {
            self.write(thing);
        }
    }

    pub(crate) fn type_arguments(
        &mut self,
        parameters: &[TypeParameterId],
        arguments: &TypeArguments,
    ) {
        for (index, &param) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }

            match arguments.get(param) {
                Some(TypeRef::Placeholder(id))
                    if id.value(self.db).is_none() =>
                {
                    // Placeholders without values aren't useful to show to the
                    // developer, so we show the type parameter instead.
                    //
                    // The parameter itself may be assigned a value through the
                    // type context (e.g. when a type is nested such as
                    // `Array[Array[T]]`), and we don't want to display that
                    // assignment as it's only to be used for the outer most
                    // type. As such, we don't use format_type() here.
                    param.format_type_without_argument(self);
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

        self.write(" (");

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
        } else {
            buffer.write("?");
        }
    }
}

impl TypeParameterId {
    fn format_type_without_argument(&self, buffer: &mut TypeFormatter) {
        let param = self.get(buffer.db);

        buffer.write(&param.name);

        if param.mutable {
            buffer.write(": mut");
        }
    }
}

impl FormatType for TypeParameterId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        // Formatting type parameters is a bit tricky, as they may be assigned
        // to themselves directly or through a placeholder. The below code isn't
        // going to win any awards, but it should ensure we don't blow the stack
        // when trying to format recursive type parameters, such as
        // `T -> placeholder -> T`.

        if let Some(arg) = buffer.type_arguments.and_then(|a| a.get(*self)) {
            if let TypeRef::Placeholder(p) = arg {
                match p.value(buffer.db) {
                    Some(t) if t.as_type_parameter() == Some(*self) => {
                        self.format_type_without_argument(buffer)
                    }
                    Some(t) => t.format_type(buffer),
                    None => self.format_type_without_argument(buffer),
                }

                return;
            }

            if arg.as_type_parameter() == Some(*self) {
                self.format_type_without_argument(buffer);
                return;
            }

            arg.format_type(buffer);
        } else {
            self.format_type_without_argument(buffer);
        };
    }
}

impl FormatType for TraitId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
    }
}

impl FormatType for TraitInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            buffer.write(&ins_of.name);

            if ins_of.type_parameters.len() > 0 {
                let params = ins_of.type_parameters.values();
                let args = self.type_arguments(buffer.db);

                buffer.write("[");
                buffer.type_arguments(params, args);
                buffer.write("]");
            }
        });
    }
}

impl FormatType for ClassId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.write(&self.get(buffer.db).name);
    }
}

impl FormatType for ClassInstance {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        buffer.descend(|buffer| {
            let ins_of = self.instance_of.get(buffer.db);

            if ins_of.kind != ClassKind::Tuple {
                buffer.write(&ins_of.name);
            }

            if ins_of.type_parameters.len() > 0 {
                let (open, close) = if ins_of.kind == ClassKind::Tuple {
                    ("(", ")")
                } else {
                    ("[", "]")
                };

                let params = ins_of.type_parameters.values();
                let args = self.type_arguments(buffer.db);

                buffer.write(open);
                buffer.type_arguments(params, args);
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

        match block.kind {
            MethodKind::Async => buffer.write("async "),
            MethodKind::AsyncMutable => buffer.write("async mut "),
            MethodKind::Static => buffer.write("static "),
            MethodKind::Moving => buffer.write("move "),
            MethodKind::Mutable | MethodKind::Destructor => {
                buffer.write("mut ")
            }
            MethodKind::Extern => buffer.write("extern "),
            MethodKind::Instance => {}
        }

        buffer.write(&block.name);

        if block.type_parameters.len() > 0 {
            buffer.write(" [");

            for (index, param) in
                block.type_parameters.values().iter().enumerate()
            {
                if index > 0 {
                    buffer.write(", ");
                }

                param.format_type(buffer);
            }

            buffer.write("]");
        }

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

            if fun.moving {
                buffer.write("fn move");
            } else {
                buffer.write("fn");
            }

            buffer.arguments(&fun.arguments, false);
            buffer.return_type(fun.return_type);
        });
    }
}

impl FormatType for TypeRef {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeRef::Owned(id) => id.format_type(buffer),
            TypeRef::Infer(id) => id.format_type(buffer),
            TypeRef::Uni(id) => {
                buffer.write_ownership("uni ");
                id.format_type(buffer);
            }
            TypeRef::UniRef(id) => {
                buffer.write_ownership("uni ref ");
                id.format_type(buffer);
            }
            TypeRef::UniMut(id) => {
                buffer.write_ownership("uni mut ");
                id.format_type(buffer);
            }
            TypeRef::Ref(id) => {
                buffer.write_ownership("ref ");
                id.format_type(buffer);
            }
            TypeRef::Mut(id) => {
                buffer.write_ownership("mut ");
                id.format_type(buffer);
            }
            TypeRef::Never => buffer.write("Never"),
            TypeRef::Any => buffer.write("Any"),
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

impl FormatType for TypeId {
    fn format_type(&self, buffer: &mut TypeFormatter) {
        match self {
            TypeId::Class(id) => id.format_type(buffer),
            TypeId::Trait(id) => id.format_type(buffer),
            TypeId::Module(id) => id.format_type(buffer),
            TypeId::ClassInstance(ins) => ins.format_type(buffer),
            TypeId::TraitInstance(id) => id.format_type(buffer),
            TypeId::TypeParameter(id) => id.format_type(buffer),
            TypeId::RigidTypeParameter(id) => {
                id.format_type_without_argument(buffer);
            }
            TypeId::Closure(id) => id.format_type(buffer),
            TypeId::Foreign(ForeignType::Int(size, true)) => {
                buffer.write(&format!("Int{}", size))
            }
            TypeId::Foreign(ForeignType::Int(size, false)) => {
                buffer.write(&format!("UInt{}", size))
            }
            TypeId::Foreign(ForeignType::Float(size)) => {
                buffer.write(&format!("Float{}", size))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Block, Class, ClassInstance, ClassKind, Closure, Database, Method,
        MethodKind, Module, ModuleId, ModuleName, Trait, TraitInstance,
        TypeArguments, TypeId, TypeParameter, TypeRef, Visibility,
    };

    #[test]
    fn test_trait_instance_format_type_with_regular_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "A".to_string(),
            Visibility::Private,
            ModuleId(0),
        );
        let trait_ins = TraitInstance::new(trait_id);

        assert_eq!(format_type(&db, trait_ins), "A".to_string());
    }

    #[test]
    fn test_trait_instance_format_type_with_generic_trait() {
        let mut db = Database::new();
        let trait_id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = trait_id.new_type_parameter(&mut db, "A".to_string());

        trait_id.new_type_parameter(&mut db, "B".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let trait_ins = TraitInstance::generic(&mut db, trait_id, targs);

        assert_eq!(format_type(&db, trait_ins), "ToString[Any, B]");
    }

    #[test]
    fn test_method_id_format_type_with_instance_method() {
        let mut db = Database::new();
        let class_a = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_b = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_d = Class::alloc(
            &mut db,
            "D".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Instance,
        );

        let ins_a =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_a)));

        let ins_b =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_b)));

        let ins_d =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_d)));

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b);
        block.set_return_type(&mut db, ins_d);

        assert_eq!(format_type(&db, block), "fn foo (a: A, b: B) -> D");
    }

    #[test]
    fn test_method_id_format_type_with_moving_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Moving,
        );

        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn move foo -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_type_parameters() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        block.new_type_parameter(&mut db, "A".to_string());
        block.new_type_parameter(&mut db, "B".to_string());
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn static foo [A, B] -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_static_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Static,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::Any,
            TypeRef::Any,
        );
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn static foo (a: Any) -> Any");
    }

    #[test]
    fn test_method_id_format_type_with_async_method() {
        let mut db = Database::new();
        let block = Method::alloc(
            &mut db,
            ModuleId(0),
            "foo".to_string(),
            Visibility::Private,
            MethodKind::Async,
        );

        block.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::Any,
            TypeRef::Any,
        );
        block.set_return_type(&mut db, TypeRef::Any);

        assert_eq!(format_type(&db, block), "fn async foo (a: Any) -> Any");
    }

    #[test]
    fn test_closure_id_format_type_never_returns() {
        let mut db = Database::new();
        let block = Closure::alloc(&mut db, false);

        block.set_return_type(&mut db, TypeRef::Never);

        assert_eq!(format_type(&db, block), "fn -> Never");
    }

    #[test]
    fn test_type_id_format_type_with_class() {
        let mut db = Database::new();
        let id = TypeId::Class(Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        ));

        assert_eq!(format_type(&db, id), "String");
    }

    #[test]
    fn test_type_id_format_type_with_trait() {
        let mut db = Database::new();
        let id = TypeId::Trait(Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
        ));

        assert_eq!(format_type(&db, id), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_module() {
        let mut db = Database::new();
        let id = TypeId::Module(Module::alloc(
            &mut db,
            ModuleName::new("foo::bar"),
            "foo/bar.inko".into(),
        ));

        assert_eq!(format_type(&db, id), "foo::bar");
    }

    #[test]
    fn test_type_id_format_type_with_class_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let ins = TypeId::ClassInstance(ClassInstance::new(id));

        assert_eq!(format_type(&db, ins), "String");
    }

    #[test]
    fn test_type_id_format_type_with_tuple_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "MyTuple".to_string(),
            ClassKind::Tuple,
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = id.new_type_parameter(&mut db, "A".to_string());
        let param2 = id.new_type_parameter(&mut db, "B".to_string());
        let mut args = TypeArguments::new();

        args.assign(param1, TypeRef::Any);
        args.assign(param2, TypeRef::Never);

        let ins =
            TypeId::ClassInstance(ClassInstance::generic(&mut db, id, args));

        assert_eq!(format_type(&db, ins), "(Any, Never)");
    }

    #[test]
    fn test_type_id_format_type_with_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToString".to_string(),
            Visibility::Private,
            ModuleId(0),
        );
        let ins = TypeId::TraitInstance(TraitInstance::new(id));

        assert_eq!(format_type(&db, ins), "ToString");
    }

    #[test]
    fn test_type_id_format_type_with_generic_class_instance() {
        let mut db = Database::new();
        let id = Class::alloc(
            &mut db,
            "Thing".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let ins =
            TypeId::ClassInstance(ClassInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "Thing[Any, E]");
    }

    #[test]
    fn test_type_id_format_type_with_generic_trait_instance() {
        let mut db = Database::new();
        let id = Trait::alloc(
            &mut db,
            "ToFoo".to_string(),
            Visibility::Private,
            ModuleId(0),
        );
        let param1 = id.new_type_parameter(&mut db, "T".to_string());

        id.new_type_parameter(&mut db, "E".to_string());

        let mut targs = TypeArguments::new();

        targs.assign(param1, TypeRef::Any);

        let ins =
            TypeId::TraitInstance(TraitInstance::generic(&mut db, id, targs));

        assert_eq!(format_type(&db, ins), "ToFoo[Any, E]");
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
        );
        let param_ins = TypeId::TypeParameter(param);
        let to_string_ins = TraitInstance::new(to_string);

        param.add_requirements(&mut db, vec![to_string_ins]);

        assert_eq!(format_type(&db, param_ins), "T");
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
        );
        let param_ins = TypeId::RigidTypeParameter(param);
        let to_string_ins = TraitInstance::new(to_string);

        param.add_requirements(&mut db, vec![to_string_ins]);

        assert_eq!(format_type(&db, param_ins), "T");
    }

    #[test]
    fn test_type_id_format_type_with_closure() {
        let mut db = Database::new();
        let class_a = Class::alloc(
            &mut db,
            "A".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_b = Class::alloc(
            &mut db,
            "B".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let class_d = Class::alloc(
            &mut db,
            "D".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let block = Closure::alloc(&mut db, true);

        let ins_a =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_a)));

        let ins_b =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_b)));

        let ins_d =
            TypeRef::Owned(TypeId::ClassInstance(ClassInstance::new(class_d)));

        block.new_argument(&mut db, "a".to_string(), ins_a, ins_a);
        block.new_argument(&mut db, "b".to_string(), ins_b, ins_b);
        block.set_return_type(&mut db, ins_d);

        let block_ins = TypeId::Closure(block);

        assert_eq!(format_type(&db, block_ins), "fn move (A, B) -> D");
    }

    #[test]
    fn test_type_ref_type_name() {
        let mut db = Database::new();
        let string = Class::alloc(
            &mut db,
            "String".to_string(),
            ClassKind::Regular,
            Visibility::Private,
            ModuleId(0),
        );
        let string_ins = TypeId::ClassInstance(ClassInstance::new(string));
        let param = TypeId::TypeParameter(TypeParameter::alloc(
            &mut db,
            "T".to_string(),
        ));

        assert_eq!(
            format_type(&db, TypeRef::Owned(string_ins)),
            "String".to_string()
        );
        assert_eq!(
            format_type(&db, TypeRef::Uni(string_ins)),
            "uni String".to_string()
        );
        assert_eq!(
            format_type(&db, TypeRef::UniMut(string_ins)),
            "uni mut String".to_string()
        );
        assert_eq!(
            format_type(&db, TypeRef::UniRef(string_ins)),
            "uni ref String".to_string()
        );
        assert_eq!(format_type(&db, TypeRef::Infer(param)), "T".to_string());
        assert_eq!(
            format_type(&db, TypeRef::Ref(string_ins)),
            "ref String".to_string()
        );
        assert_eq!(format_type(&db, TypeRef::Never), "Never".to_string());
        assert_eq!(format_type(&db, TypeRef::Any), "Any".to_string());
        assert_eq!(format_type(&db, TypeRef::Error), "<error>".to_string());
        assert_eq!(format_type(&db, TypeRef::Unknown), "<unknown>".to_string());
    }

    #[test]
    fn test_ctype_format() {
        let mut db = Database::new();
        let foo = Class::alloc(
            &mut db,
            "Foo".to_string(),
            ClassKind::Extern,
            Visibility::Public,
            ModuleId(0),
        );

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
                TypeRef::pointer(TypeId::Foreign(ForeignType::Int(8, true)))
            ),
            "Pointer[Int8]"
        );
        assert_eq!(
            format_type(
                &db,
                TypeRef::pointer(TypeId::Foreign(ForeignType::Int(8, false)))
            ),
            "Pointer[UInt8]"
        );
        assert_eq!(format_type(&db, TypeRef::foreign_struct(foo)), "Foo");
    }
}
