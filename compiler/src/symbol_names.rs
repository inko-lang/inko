//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use types::{ClassId, ConstantId, Database, MethodId, ModuleId};

pub(crate) const SYMBOL_PREFIX: &str = "_I";

pub(crate) fn class_name(db: &Database, id: ClassId) -> String {
    format!("{}#{}", id.name(db), id.0)
}

pub(crate) fn method_name(db: &Database, id: MethodId) -> String {
    format!("{}#{}", id.name(db), id.0)
}

fn mangled_method_name(db: &Database, method: MethodId) -> String {
    let class = method.receiver(db).class_id(db).unwrap();
    let mod_name = method.module(db).name(db).as_str();

    // Method names include their IDs to ensure specialized methods with the
    // same name and type don't conflict with each other.
    if class.kind(db).is_module() {
        // This ensures that methods such as `std::process.sleep` aren't
        // formatted as `std::process::std::process.sleep`. This in turn makes
        // stack traces easier to read.
        format!("{}M_{}.{}", SYMBOL_PREFIX, mod_name, method_name(db, method))
    } else {
        format!(
            "{}M_{}.{}.{}",
            SYMBOL_PREFIX,
            mod_name,
            class.name(db),
            method_name(db, method)
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
