//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use types::{ClassId, ConstantId, Database, MethodId, ModuleId, Shape};

pub(crate) const SYMBOL_PREFIX: &str = "_I";

pub(crate) fn shapes(shapes: &[Shape]) -> String {
    shapes.iter().fold(String::new(), |res, shape| res + shape.identifier())
}

pub(crate) fn class_name(db: &Database, id: ClassId) -> String {
    format!("{}#{}", id.name(db), shapes(id.shapes(db)))
}

pub(crate) fn method_name(
    db: &Database,
    class: ClassId,
    id: MethodId,
) -> String {
    format!(
        "{}#{}{}",
        id.name(db),
        shapes(class.shapes(db)),
        shapes(id.shapes(db)),
    )
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
                    "{}T_{}.{}",
                    SYMBOL_PREFIX,
                    module.id.name(db).as_str(),
                    class_name(db, class)
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
