//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use std::fmt::Write as _;
use types::{
    Block, ClosureSelfType, ConstantId, Database, ForeignType, MethodId,
    ModuleId, Sign, TraitId, TypeArguments, TypeEnum, TypeId, TypeRef,
    FLOAT_ID, INT_ID,
};

pub(crate) const SYMBOL_PREFIX: &str = "_I";

/// The name of the global variable that stores the runtime state.
pub(crate) const STATE_GLOBAL: &str = "_IG_INKO_STATE";

/// The name of the global variable that stores the stack mask.
pub(crate) const STACK_MASK_GLOBAL: &str = "_IG_INKO_STACK_MASK";

pub(crate) fn format_type_enum(db: &Database, typ: TypeEnum, buf: &mut String) {
    match typ {
        TypeEnum::TypeInstance(t) => {
            let tid = t.instance_of();

            // This ensures that Int64 and Int, and Float64 and Float produce
            // the same symbol names.
            match tid.0 {
                INT_ID => buf.push_str("i64"),
                FLOAT_ID => buf.push_str("f64"),
                _ => {
                    let _ = write!(buf, "{}.", tid.module(db).name(db));

                    format_type_name(db, tid, buf);
                }
            }
        }
        TypeEnum::TraitInstance(t) => {
            let tid = t.instance_of();
            let _ = write!(buf, "{}.", tid.module(db).name(db));

            format_trait_name(db, tid, buf);
        }
        // While closure _values_ are turned into regular type instances,
        // closure _types_ (e.g. used in a method argument's signature) remain a
        // dedicated type.
        TypeEnum::Closure(t) => {
            buf.push_str(if t.captures_by_moving(db) {
                "fn move"
            } else {
                "fn"
            });

            if t.number_of_arguments(db) > 0 {
                buf.push_str(" (");

                for (idx, arg) in t.arguments(db).into_iter().enumerate() {
                    if idx > 0 {
                        buf.push_str(", ");
                    }

                    format_type(db, arg.value_type, buf);
                }

                buf.push(')');
            }

            match t.return_type(db) {
                ret if ret == TypeRef::nil() => {}
                ret => {
                    buf.push_str(" -> ");
                    format_type(db, ret, buf)
                }
            }
        }
        TypeEnum::Foreign(ForeignType::Float(bits)) => {
            let _ = write!(buf, "f{}", bits);
        }
        TypeEnum::Foreign(ForeignType::Int(bits, Sign::Signed)) => {
            let _ = write!(buf, "i{}", bits);
        }
        TypeEnum::Foreign(ForeignType::Int(bits, Sign::Unsigned)) => {
            let _ = write!(buf, "u{}", bits);
        }
        // Other types (e.g. type parameters or modules) can't occur at this
        // point.
        _ => unreachable!(),
    }
}

pub(crate) fn format_type(db: &Database, typ: TypeRef, buf: &mut String) {
    let (label, subj) = match typ {
        TypeRef::Owned(t) => ("", t),
        TypeRef::Uni(t) => ("uni ", t),
        TypeRef::Ref(t) => ("ref ", t),
        TypeRef::Mut(t) => ("mut ", t),
        TypeRef::UniRef(t) => ("uni ref ", t),
        TypeRef::UniMut(t) => ("uni mut ", t),
        TypeRef::Pointer(t) => {
            buf.push_str("Pointer[");
            format_type_enum(db, t, buf);
            buf.push(']');
            return;
        }
        TypeRef::Unknown => {
            // Placeholders that aren't assigned types are replaced with Unknown
            // as part of specialization. We can encounter such cases when a
            // generic type has a static method that doesn't use the type's type
            // parameters.
            buf.push('?');
            return;
        }
        TypeRef::Never => {
            buf.push_str("Never");
            return;
        }
        // Other types can't be present at this point, outside of any compiler
        // bugs. Most notably, placeholders are replaced with the types they're
        // assigned to.
        _ => unreachable!("{:?} must be specialized into some other type", typ),
    };

    let write_label = match subj {
        TypeEnum::TypeInstance(i) => {
            !matches!(i.instance_of().0, INT_ID | FLOAT_ID)
        }
        TypeEnum::Foreign(_) => false,
        _ => true,
    };

    if write_label && !label.is_empty() {
        buf.push_str(label);
    }

    format_type_enum(db, subj, buf);
}

pub(crate) fn format_types(db: &Database, types: &[TypeRef], buf: &mut String) {
    for &typ in types {
        format_type(db, typ, buf);
    }
}

fn format_type_base_name(db: &Database, id: TypeId, buf: &mut String) {
    buf.push_str(id.name(db));

    // For closures the process of generating a name is a little more tricky: a
    // default method may define a closure. If that default method is inherited
    // as-is by different types, the implementations of that default method
    // should all use a unique closure type. If these methods instead share a
    // common closure type, we end up generating incorrect code.
    //
    // To prevent this from happening, the closure name includes the source
    // location and the type of `self` of the method the closure is defined in.
    // The symbol name still includes the module the closure is originally
    // defined in, such that if the module that includes the implementation
    // _also_ defines a closure at the same location, the symbol names don't
    // collide.
    if !id.is_closure(db) {
        return;
    }

    let loc = id.location(db);
    let stype = id.self_type_for_closure(db).unwrap();
    let sname = match stype {
        ClosureSelfType::TypeInstance(t) => {
            qualified_type_name(db, t.instance_of().module(db), t.instance_of())
        }
        ClosureSelfType::Type(t) => qualified_type_name(db, t.module(db), t),
        ClosureSelfType::Module(t) => t.name(db).to_string(),
    };

    // The exact format used here doesn't really matter, but we try to keep it
    // somewhat readable for use in external tooling (e.g. a profiler that
    // doesn't support demangling our format).
    buf.push_str(&format!(
        "({},{},{})",
        sname, loc.line_start, loc.column_start,
    ));
}

fn format_type_arguments(
    db: &Database,
    arguments: &TypeArguments,
    buf: &mut String,
) {
    buf.push('[');

    for (idx, typ) in arguments.values().enumerate() {
        if idx > 0 {
            buf.push_str(", ");
        }

        format_type(db, typ, buf);
    }

    buf.push(']');
}

pub(crate) fn format_type_name(db: &Database, id: TypeId, buf: &mut String) {
    format_type_base_name(db, id, buf);

    if let Some(args) = id.type_arguments(db) {
        format_type_arguments(db, args, buf);
    }
}

pub(crate) fn format_trait_name(db: &Database, id: TraitId, buf: &mut String) {
    buf.push_str(id.name(db));

    if let Some(args) = id.type_arguments(db) {
        format_type_arguments(db, args, buf);
    }
}

pub(crate) fn qualified_type_name(
    db: &Database,
    module: ModuleId,
    tid: TypeId,
) -> String {
    let mut name = format!("{}.", module.name(db));

    format_type_name(db, tid, &mut name);
    name
}

pub(crate) fn format_method_name(
    db: &Database,
    tid: TypeId,
    id: MethodId,
    buf: &mut String,
) {
    buf.push_str(id.name(db));

    let cargs = tid.type_arguments(db);
    let margs = id.type_arguments(db);

    if cargs.is_some() || !margs.is_empty() {
        buf.push_str("#[");

        for (idx, typ) in cargs
            .iter()
            .flat_map(|t| t.values())
            .chain(margs.iter().cloned())
            .enumerate()
        {
            if idx > 0 {
                buf.push_str(", ");
            }

            format_type(db, typ, buf);
        }

        buf.push(']');
    }
}

fn mangled_method_name(db: &Database, method: MethodId) -> String {
    let tid = method.receiver(db).type_id(db).unwrap();

    // We don't use MethodId::source_module() here as for default methods that
    // may point to the module that defined the trait, rather than the module
    // the trait is implemented in. That could result in symbol name conflicts
    // when two different modules implement the same trait.
    let mod_name = method.module(db).method_symbol_name(db).as_str();

    // For closures we use a dedicated prefix such that when a stacktrace is
    // generated, we don't include the generated names because these aren't
    // useful for debugging purposes.
    let mut name = format!(
        "{}{}_{}.",
        SYMBOL_PREFIX,
        if tid.is_closure(db) { "MC" } else { "M" },
        mod_name
    );

    // This ensures that methods such as `std.process.sleep` aren't formatted
    // as `std.process.std.process.sleep`. This in turn makes stack traces
    // easier to read.
    if !tid.kind(db).is_module() {
        format_type_base_name(db, tid, &mut name);
        name.push('.');
    }

    format_method_name(db, tid, method, &mut name);
    name
}

/// A cache of mangled symbol names.
pub(crate) struct SymbolNames {
    pub(crate) types: HashMap<TypeId, String>,
    pub(crate) methods: HashMap<MethodId, String>,
    pub(crate) constants: HashMap<ConstantId, String>,
    pub(crate) setup_types: HashMap<ModuleId, String>,
    pub(crate) setup_constants: HashMap<ModuleId, String>,
}

impl SymbolNames {
    pub(crate) fn new(db: &Database, mir: &Mir) -> Self {
        let mut types = HashMap::new();
        let mut methods = HashMap::new();
        let mut constants = HashMap::new();
        let mut setup_types = HashMap::new();
        let mut setup_constants = HashMap::new();

        for module in mir.modules.values() {
            for &typ in &module.types {
                let tname = format!(
                    "{}T_{}",
                    SYMBOL_PREFIX,
                    qualified_type_name(db, module.id, typ)
                );

                types.insert(typ, tname);
            }
        }

        for &method in mir.methods.keys() {
            methods.insert(method, mangled_method_name(db, method));
        }

        for id in mir.constants.keys() {
            let mod_name = id.module(db).name(db).as_str();
            let name = id.name(db);

            constants.insert(
                *id,
                format!("{}C_{}.{}", SYMBOL_PREFIX, mod_name, name),
            );
        }

        for &id in mir.modules.keys() {
            let mod_name = id.name(db).as_str();
            let types = format!("{}M_{}.$types", SYMBOL_PREFIX, mod_name);
            let constants =
                format!("{}M_{}.$constants", SYMBOL_PREFIX, mod_name);

            setup_types.insert(id, types);
            setup_constants.insert(id, constants);
        }

        Self { types, methods, constants, setup_types, setup_constants }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use location::Location;
    use types::module_name::ModuleName;
    use types::{
        Closure, ClosureId, Method, MethodKind, Module, Trait, TraitInstance,
        Type, TypeInstance, TypeKind, TypeParameter, TypeParameterId,
        Visibility,
    };

    fn name(db: &Database, typ: TypeRef) -> String {
        let mut buf = String::new();

        format_type(db, typ, &mut buf);
        buf
    }

    fn arguments(pairs: &[(TypeParameterId, TypeRef)]) -> TypeArguments {
        let mut args = TypeArguments::new();

        for &(par, typ) in pairs {
            args.assign(par, typ);
        }

        args
    }

    fn instance(of: TypeId) -> TypeEnum {
        TypeEnum::TypeInstance(TypeInstance::new(of))
    }

    fn trait_instance(of: TraitId) -> TypeEnum {
        TypeEnum::TraitInstance(TraitInstance::new(of))
    }

    fn closure(id: ClosureId) -> TypeEnum {
        TypeEnum::Closure(id)
    }

    #[test]
    fn test_format_type() {
        let mut db = Database::new();
        let mid =
            Module::alloc(&mut db, ModuleName::new("a.b.c"), "c.inko".into());
        let str_mod = Module::alloc(
            &mut db,
            ModuleName::new("std.string"),
            "string.inko".into(),
        );
        let kind = TypeKind::Regular;
        let vis = Visibility::Public;
        let loc = Location::default();
        let cls1 = Type::alloc(&mut db, "A".to_string(), kind, vis, mid, loc);
        let cls2 = Type::alloc(&mut db, "B".to_string(), kind, vis, mid, loc);
        let tid1 = Trait::alloc(&mut db, "A".to_string(), vis, mid, loc);
        let tid2 = Trait::alloc(&mut db, "B".to_string(), vis, mid, loc);
        let par1 = TypeParameter::alloc(&mut db, "T1".to_string());
        let par2 = TypeParameter::alloc(&mut db, "T2".to_string());
        let fn_norm = Closure::alloc(&mut db, false);
        let fn_move = Closure::alloc(&mut db, true);
        let fn_typ = Type::alloc(
            &mut db,
            "Closure123".to_string(),
            TypeKind::Closure,
            vis,
            mid,
            loc,
        );

        fn_norm.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::int(),
            TypeRef::int(),
            loc,
        );
        fn_norm.new_argument(
            &mut db,
            "b".to_string(),
            TypeRef::float(),
            TypeRef::float(),
            loc,
        );
        fn_norm.set_return_type(&mut db, TypeRef::string());
        fn_move.new_argument(
            &mut db,
            "a".to_string(),
            TypeRef::int(),
            TypeRef::int(),
            loc,
        );
        fn_move.set_return_type(&mut db, TypeRef::nil());
        TypeId::string().set_module(&mut db, str_mod);
        cls1.set_type_arguments(
            &mut db,
            arguments(&[
                (par1, TypeRef::foreign_signed_int(64)),
                (par2, TypeRef::Owned(instance(cls2))),
            ]),
        );
        cls2.set_type_arguments(
            &mut db,
            arguments(&[(par1, TypeRef::string())]),
        );
        tid2.set_type_arguments(
            &mut db,
            arguments(&[(par1, TypeRef::string()), (par2, TypeRef::int())]),
        );
        fn_typ.set_type_arguments(
            &mut db,
            arguments(&[(par1, TypeRef::string()), (par2, TypeRef::int())]),
        );
        fn_typ.set_self_type_for_closure(
            &mut db,
            instance(TypeId::string()).into(),
        );

        assert_eq!(name(&db, TypeRef::int()), "i64");
        assert_eq!(name(&db, TypeRef::foreign_signed_int(64)), "i64");
        assert_eq!(name(&db, TypeRef::foreign_signed_int(16)), "i16");
        assert_eq!(name(&db, TypeRef::foreign_unsigned_int(64)), "u64");
        assert_eq!(name(&db, TypeRef::foreign_unsigned_int(16)), "u16");
        assert_eq!(name(&db, TypeRef::float()), "f64");
        assert_eq!(name(&db, TypeRef::foreign_float(64)), "f64");
        assert_eq!(name(&db, TypeRef::foreign_float(32)), "f32");
        assert_eq!(
            name(&db, TypeRef::Owned(instance(cls1))),
            "a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::Ref(instance(cls1))),
            "ref a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::Mut(instance(cls1))),
            "mut a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::Uni(instance(cls1))),
            "uni a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::UniRef(instance(cls1))),
            "uni ref a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::UniMut(instance(cls1))),
            "uni mut a.b.c.A[i64, a.b.c.B[std.string.String]]"
        );
        assert_eq!(
            name(&db, TypeRef::Pointer(instance(cls1))),
            "Pointer[a.b.c.A[i64, a.b.c.B[std.string.String]]]"
        );
        assert_eq!(name(&db, TypeRef::Owned(trait_instance(tid1))), "a.b.c.A");
        assert_eq!(
            name(&db, TypeRef::Owned(trait_instance(tid2))),
            "a.b.c.B[std.string.String, i64]"
        );
        assert_eq!(
            name(&db, TypeRef::Owned(closure(fn_norm))),
            "fn (i64, f64) -> std.string.String"
        );
        assert_eq!(
            name(&db, TypeRef::Owned(closure(fn_move))),
            "fn move (i64)"
        );
        assert_eq!(
            name(&db, TypeRef::Owned(instance(fn_typ))),
            "a.b.c.Closure123(std.string.String,1,1)[std.string.String, i64]"
        );
    }

    #[test]
    fn test_mangled_method_name() {
        let mut db = Database::new();
        let mid = Module::alloc(
            &mut db,
            ModuleName::new("std.array"),
            "array.inko".into(),
        );
        let typ = TypeId::array();
        let vis = Visibility::Public;
        let loc = Location::default();
        let meth = Method::alloc(
            &mut db,
            mid,
            loc,
            "example".to_string(),
            vis,
            MethodKind::Instance,
        );
        let par = typ.new_type_parameter(&mut db, "T".to_string());

        typ.set_module(&mut db, mid);
        typ.set_type_arguments(&mut db, arguments(&[(par, TypeRef::int())]));
        meth.set_type_arguments(&mut db, vec![TypeRef::float()]);
        meth.set_receiver(&mut db, TypeRef::Owned(instance(typ)));

        assert_eq!(
            mangled_method_name(&db, meth),
            "_IM_std.array.Array.example#[i64, f64]"
        );
    }

    #[test]
    fn test_mangled_method_name_with_module_method() {
        let mut db = Database::new();
        let mid = Module::alloc(
            &mut db,
            ModuleName::new("std.array"),
            "array.inko".into(),
        );
        let vis = Visibility::Public;
        let loc = Location::default();
        let meth = Method::alloc(
            &mut db,
            mid,
            loc,
            "example".to_string(),
            vis,
            MethodKind::Instance,
        );

        meth.set_type_arguments(&mut db, vec![TypeRef::float()]);
        meth.set_receiver(&mut db, TypeRef::Owned(TypeEnum::Module(mid)));

        assert_eq!(
            mangled_method_name(&db, meth),
            "_IM_std.array.example#[f64]"
        );
    }

    #[test]
    fn test_mangled_method_name_with_closure() {
        let mut db = Database::new();
        let mid = Module::alloc(
            &mut db,
            ModuleName::new("std.int"),
            "int.inko".into(),
        );
        let vis = Visibility::Public;
        let loc = Location::new(&(1..=1), &(10..=10));
        let typ = Type::alloc(
            &mut db,
            "Closure123".to_string(),
            TypeKind::Closure,
            vis,
            mid,
            loc,
        );
        let meth = Method::alloc(
            &mut db,
            mid,
            loc,
            "call".to_string(),
            vis,
            MethodKind::Instance,
        );

        typ.set_module(&mut db, mid);
        typ.set_self_type_for_closure(
            &mut db,
            TypeEnum::TypeInstance(TypeInstance::new(TypeId::int())).into(),
        );
        meth.set_receiver(&mut db, TypeRef::Owned(instance(typ)));

        assert_eq!(
            mangled_method_name(&db, meth),
            "_IMC_std.int.Closure123(std.int.Int,1,10).call"
        );
    }

    #[test]
    fn test_mangled_method_name_with_closure_in_static_method() {
        let mut db = Database::new();
        let mid = Module::alloc(
            &mut db,
            ModuleName::new("std.int"),
            "int.inko".into(),
        );
        let vis = Visibility::Public;
        let loc = Location::new(&(1..=1), &(10..=10));
        let typ = Type::alloc(
            &mut db,
            "Closure123".to_string(),
            TypeKind::Closure,
            vis,
            mid,
            loc,
        );
        let meth = Method::alloc(
            &mut db,
            mid,
            loc,
            "call".to_string(),
            vis,
            MethodKind::Instance,
        );

        typ.set_module(&mut db, mid);
        typ.set_self_type_for_closure(
            &mut db,
            TypeEnum::Type(TypeId::int()).into(),
        );
        meth.set_receiver(&mut db, TypeRef::Owned(instance(typ)));

        assert_eq!(
            mangled_method_name(&db, meth),
            "_IMC_std.int.Closure123(std.int.Int,1,10).call"
        );
    }

    #[test]
    fn test_mangled_method_name_with_closure_in_module_method() {
        let mut db = Database::new();
        let mid = Module::alloc(
            &mut db,
            ModuleName::new("std.int"),
            "int.inko".into(),
        );
        let vis = Visibility::Public;
        let loc = Location::new(&(1..=1), &(10..=10));
        let typ = Type::alloc(
            &mut db,
            "Closure123".to_string(),
            TypeKind::Closure,
            vis,
            mid,
            loc,
        );
        let meth = Method::alloc(
            &mut db,
            mid,
            loc,
            "call".to_string(),
            vis,
            MethodKind::Instance,
        );

        typ.set_module(&mut db, mid);
        typ.set_self_type_for_closure(&mut db, TypeEnum::Module(mid).into());
        meth.set_receiver(&mut db, TypeRef::Owned(instance(typ)));

        assert_eq!(
            mangled_method_name(&db, meth),
            "_IMC_std.int.Closure123(std.int,1,10).call"
        );
    }
}
