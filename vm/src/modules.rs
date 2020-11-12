//! Collections of Inko modules.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::bytecode_parser;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::vm::state::State;
use ahash::AHashMap;

/// A collection of all Inko modules for the current program.
pub struct Modules {
    map: AHashMap<String, ObjectPointer>,
}

impl Modules {
    pub fn new() -> Self {
        Modules {
            map: AHashMap::default(),
        }
    }

    pub fn parse_image(
        &mut self,
        state: &State,
        path: &str,
    ) -> Result<(), String> {
        let modules =
            bytecode_parser::parse_file(state, path).map_err(|err| {
                format!("The bytecode image {} is invalid: {}", path, err)
            })?;

        self.add(state, modules);

        Ok(())
    }

    pub fn add(&mut self, state: &State, modules: Vec<Module>) {
        let mut alloc = state.permanent_allocator.lock();
        let proto = state.module_prototype;

        for module in modules {
            let name = module
                .name()
                .string_value()
                .expect("Module names must be String pointers")
                .to_owned_string();

            let val = object_value::module(ArcWithoutWeak::new(module));
            let ptr = alloc.allocate_with_prototype(val, proto);

            self.map.insert(name, ptr);
        }
    }

    pub fn get(&self, name: &str) -> Result<ObjectPointer, String> {
        self.map
            .get(name)
            .cloned()
            .ok_or_else(|| format!("The module {} doesn't exist", name))
    }

    pub fn list(&self) -> Vec<ObjectPointer> {
        self.map.values().copied().collect()
    }

    pub fn get_for_execution(
        &mut self,
        name: &str,
    ) -> Result<(ObjectPointer, bool), String> {
        self.get(name).and_then(|ptr| {
            let module = ptr.module_value_mut()?;

            Ok((ptr, module.mark_as_executed()))
        })
    }
}
