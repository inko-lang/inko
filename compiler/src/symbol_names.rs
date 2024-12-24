//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use std::fmt::Write as _;
use types::{ConstantId, Database, MethodId, ModuleId, Shape, Sign, TypeId};

pub(crate) const SYMBOL_PREFIX: &str = "_I";

/// The name of the global variable that stores the runtime state.
pub(crate) const STATE_GLOBAL: &str = "_IG_INKO_STATE";

/// The name of the global variable that stores the stack mask.
pub(crate) const STACK_MASK_GLOBAL: &str = "_IG_INKO_STACK_MASK";

pub(crate) fn format_shape(db: &Database, shape: Shape, buf: &mut String) {
    let _ = match shape {
        Shape::Owned => write!(buf, "o"),
        Shape::Mut => write!(buf, "m"),
        Shape::Ref => write!(buf, "r"),
        Shape::Int(s, Sign::Signed) => write!(buf, "i{}", s),
        Shape::Int(s, Sign::Unsigned) => write!(buf, "u{}", s),
        Shape::Float(s) => write!(buf, "f{}", s),
        Shape::Boolean => write!(buf, "b"),
        Shape::String => write!(buf, "s"),
        Shape::Atomic => write!(buf, "a"),
        Shape::Nil => write!(buf, "n"),
        Shape::Pointer => write!(buf, "p"),
        Shape::Copy(ins) => {
            let _ = write!(buf, "C{}.", ins.instance_of().module(db).name(db));

            format_type_name(db, ins.instance_of(), buf);
            Ok(())
        }
        Shape::Inline(ins) => {
            let _ = write!(buf, "IO{}.", ins.instance_of().module(db).name(db));

            format_type_name(db, ins.instance_of(), buf);
            Ok(())
        }
        Shape::InlineRef(ins) => {
            let _ = write!(buf, "IR{}.", ins.instance_of().module(db).name(db));

            format_type_name(db, ins.instance_of(), buf);
            Ok(())
        }
        Shape::InlineMut(ins) => {
            let _ = write!(buf, "IM{}.", ins.instance_of().module(db).name(db));

            format_type_name(db, ins.instance_of(), buf);
            Ok(())
        }
    };
}

pub(crate) fn format_shapes(db: &Database, shapes: &[Shape], buf: &mut String) {
    for &shape in shapes {
        format_shape(db, shape, buf);
    }
}

fn format_type_base_name(db: &Database, id: TypeId, name: &mut String) {
    name.push_str(id.name(db));

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
    let stype = id.specialization_key(db).self_type.unwrap().instance_of();

    // The exact format used here doesn't really matter, but we try to keep it
    // somewhat readable for use in external tooling (e.g. a profiler that
    // doesn't support demangling our format).
    name.push_str(&format!(
        "({},{},{})",
        qualified_type_name(db, stype.module(db), stype),
        loc.line_start,
        loc.column_start,
    ));
}

pub(crate) fn format_type_name(db: &Database, id: TypeId, buf: &mut String) {
    format_type_base_name(db, id, buf);

    let shapes = id.shapes(db);

    if !shapes.is_empty() {
        buf.push('#');
        format_shapes(db, shapes, buf);
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
    name: &mut String,
) {
    name.push_str(id.name(db));

    let cshapes = tid.shapes(db);
    let mshapes = id.shapes(db);

    if !cshapes.is_empty() || !mshapes.is_empty() {
        name.push('#');
        format_shapes(db, cshapes, name);
        format_shapes(db, mshapes, name);
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

    // This ensures that methods such as `std::process.sleep` aren't formatted
    // as `std::process::std::process.sleep`. This in turn makes stack traces
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
        Module, SpecializationKey, Type, TypeInstance, TypeKind, Visibility,
    };

    fn name(db: &Database, shape: Shape) -> String {
        let mut buf = String::new();

        format_shape(db, shape, &mut buf);
        buf
    }

    #[test]
    fn test_format_shape() {
        let mut db = Database::new();
        let mid =
            Module::alloc(&mut db, ModuleName::new("a.b.c"), "c.inko".into());
        let kind = TypeKind::Regular;
        let vis = Visibility::Public;
        let loc = Location::default();
        let cls1 = Type::alloc(&mut db, "A".to_string(), kind, vis, mid, loc);
        let cls2 = Type::alloc(&mut db, "B".to_string(), kind, vis, mid, loc);
        let cls3 = Type::alloc(&mut db, "C".to_string(), kind, vis, mid, loc);
        let cls4 = Type::alloc(&mut db, "D".to_string(), kind, vis, mid, loc);

        cls1.set_specialization_key(
            &mut db,
            SpecializationKey::new(vec![
                Shape::Int(64, Sign::Signed),
                Shape::Inline(TypeInstance::new(cls2)),
            ]),
        );
        cls2.set_specialization_key(
            &mut db,
            SpecializationKey::new(vec![Shape::String]),
        );
        cls3.set_specialization_key(
            &mut db,
            SpecializationKey::new(vec![Shape::InlineRef(TypeInstance::new(
                cls2,
            ))]),
        );
        cls4.set_specialization_key(
            &mut db,
            SpecializationKey::new(vec![Shape::InlineMut(TypeInstance::new(
                cls2,
            ))]),
        );

        assert_eq!(name(&db, Shape::Owned), "o");
        assert_eq!(name(&db, Shape::Mut), "m");
        assert_eq!(name(&db, Shape::Ref), "r");
        assert_eq!(name(&db, Shape::Int(32, Sign::Signed)), "i32");
        assert_eq!(name(&db, Shape::Int(32, Sign::Unsigned)), "u32");
        assert_eq!(name(&db, Shape::Float(32)), "f32");
        assert_eq!(name(&db, Shape::Boolean), "b");
        assert_eq!(name(&db, Shape::String), "s");
        assert_eq!(name(&db, Shape::Atomic), "a");
        assert_eq!(name(&db, Shape::Nil), "n");
        assert_eq!(name(&db, Shape::Pointer), "p");
        assert_eq!(
            name(&db, Shape::Inline(TypeInstance::new(cls1))),
            "IOa.b.c.A#i64IOa.b.c.B#s"
        );
        assert_eq!(
            name(&db, Shape::InlineMut(TypeInstance::new(cls3))),
            "IMa.b.c.C#IRa.b.c.B#s"
        );
        assert_eq!(
            name(&db, Shape::InlineRef(TypeInstance::new(cls4))),
            "IRa.b.c.D#IMa.b.c.B#s"
        );
    }
}
