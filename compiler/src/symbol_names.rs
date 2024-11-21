//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use std::fmt::Write as _;
use types::{ClassId, ConstantId, Database, MethodId, ModuleId, Shape, Sign};

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
        Shape::Stack(ins) => {
            let cls = ins.instance_of();
            let _ = write!(buf, "S{}.", cls.module(db).name(db));

            format_class_name(db, cls, buf);
            Ok(())
        }
    };
}

pub(crate) fn format_shapes(db: &Database, shapes: &[Shape], buf: &mut String) {
    for &shape in shapes {
        format_shape(db, shape, buf);
    }
}

pub(crate) fn format_class_name(db: &Database, id: ClassId, buf: &mut String) {
    buf.push_str(id.name(db));

    let is_stack = id.is_stack_allocated(db);
    let shapes = id.shapes(db);

    if !shapes.is_empty() || is_stack {
        buf.push('#');
    }

    // In case we infer a type to be stack allocated (or we did so in the past
    // but it's no longer the case), this ensures we flush the object cache.
    if is_stack {
        buf.push('S');
    }

    if !shapes.is_empty() {
        format_shapes(db, shapes, buf);
    }
}

pub(crate) fn qualified_class_name(
    db: &Database,
    module: ModuleId,
    class: ClassId,
) -> String {
    let mut name = format!("{}.", module.name(db));

    format_class_name(db, class, &mut name);
    name
}

pub(crate) fn method_name(
    db: &Database,
    class: ClassId,
    id: MethodId,
) -> String {
    let mut name = id.name(db).to_string();
    let cshapes = class.shapes(db);
    let mshapes = id.shapes(db);

    if !cshapes.is_empty() || !mshapes.is_empty() {
        name.push('#');
        format_shapes(db, cshapes, &mut name);
        format_shapes(db, mshapes, &mut name);
    }

    name
}

fn mangled_method_name(db: &Database, method: MethodId) -> String {
    let class = method.receiver(db).class_id(db).unwrap();

    // We don't use MethodId::source_module() here as for default methods that
    // may point to the module that defined the trait, rather than the module
    // the trait is implemented in. That could result in symbol name conflicts
    // when two different modules implement the same trait.
    let mod_name = method.module(db).method_symbol_name(db).as_str();

    // We don't use type IDs in the name as this would couple the symbol names
    // to the order in which modules are processed.
    if class.kind(db).is_module() {
        // This ensures that methods such as `std::process.sleep` aren't
        // formatted as `std::process::std::process.sleep`. This in turn makes
        // stack traces easier to read.
        format!(
            "{}M_{}.{}",
            SYMBOL_PREFIX,
            mod_name,
            method_name(db, class, method)
        )
    } else {
        format!(
            "{}M_{}.{}.{}",
            SYMBOL_PREFIX,
            mod_name,
            class.name(db),
            method_name(db, class, method)
        )
    }
}

/// A cache of mangled symbol names.
pub(crate) struct SymbolNames {
    pub(crate) classes: HashMap<ClassId, String>,
    pub(crate) methods: HashMap<MethodId, String>,
    pub(crate) constants: HashMap<ConstantId, String>,
    pub(crate) setup_classes: HashMap<ModuleId, String>,
    pub(crate) setup_constants: HashMap<ModuleId, String>,
}

impl SymbolNames {
    pub(crate) fn new(db: &Database, mir: &Mir) -> Self {
        let mut classes = HashMap::new();
        let mut methods = HashMap::new();
        let mut constants = HashMap::new();
        let mut setup_classes = HashMap::new();
        let mut setup_constants = HashMap::new();

        for module in mir.modules.values() {
            for &class in &module.classes {
                let class_name = format!(
                    "{}T_{}",
                    SYMBOL_PREFIX,
                    qualified_class_name(db, module.id, class)
                );

                classes.insert(class, class_name);
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
            let classes = format!("{}M_{}.$classes", SYMBOL_PREFIX, mod_name);
            let constants =
                format!("{}M_{}.$constants", SYMBOL_PREFIX, mod_name);

            setup_classes.insert(id, classes);
            setup_constants.insert(id, constants);
        }

        Self { classes, methods, constants, setup_classes, setup_constants }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use location::Location;
    use types::module_name::ModuleName;
    use types::{Class, ClassInstance, ClassKind, Module, Visibility};

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
        let kind = ClassKind::Regular;
        let vis = Visibility::Public;
        let loc = Location::default();
        let cls1 = Class::alloc(&mut db, "A".to_string(), kind, vis, mid, loc);
        let cls2 = Class::alloc(&mut db, "B".to_string(), kind, vis, mid, loc);

        cls1.set_shapes(
            &mut db,
            vec![
                Shape::Int(64, Sign::Signed),
                Shape::Stack(ClassInstance::new(cls2)),
            ],
        );
        cls2.set_shapes(&mut db, vec![Shape::String]);

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
            name(&db, Shape::Stack(ClassInstance::new(cls1))),
            "Sa.b.c.A#i64Sa.b.c.B#s"
        );
    }
}
