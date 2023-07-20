//! Mangled symbol names for native code.
use crate::mir::Mir;
use std::collections::HashMap;
use types::{ClassId, ConstantId, Database, MethodId, ModuleId};

pub(crate) const SYMBOL_PREFIX: &str = "_I";

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
        let prefix = SYMBOL_PREFIX;

        for module_index in 0..mir.modules.len() {
            let module = &mir.modules[module_index];
            let mod_name = module.id.name(db).as_str();

            for &class in &module.classes {
                let is_mod = class.kind(db).is_module();
                let class_name =
                    format!("{}T_{}.{}", prefix, mod_name, class.name(db));

                classes.insert(class, class_name);

                for &method in &mir.classes[&class].methods {
                    let name = if is_mod {
                        // This ensures that methods such as
                        // `std::process.sleep` aren't formatted as
                        // `std::process::std::process.sleep`. This in turn
                        // makes stack traces easier to read.
                        format!("{}M_{}.{}", prefix, mod_name, method.name(db))
                    } else {
                        format!(
                            "{}M_{}.{}.{}",
                            prefix,
                            mod_name,
                            class.name(db),
                            method.name(db)
                        )
                    };

                    methods.insert(method, name);
                }
            }
        }

        for id in mir.constants.keys() {
            let mod_name = id.module(db).name(db).as_str();
            let name = id.name(db);

            constants.insert(*id, format!("{}C_{}.{}", prefix, mod_name, name));
        }

        for &id in mir.modules.keys() {
            let mod_name = id.name(db).as_str();
            let classes = format!("{}M_{}.$classes", prefix, mod_name);
            let constants = format!("{}M_{}.$constants", prefix, mod_name);

            setup_classes.insert(id, classes);
            setup_constants.insert(id, constants);
        }

        Self { classes, methods, constants, setup_classes, setup_constants }
    }
}
