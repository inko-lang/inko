//! Parsing and caching of bytecode modules.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::bytecode_parser;
use crate::compiled_code::CompiledCode;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::vm::state::RcState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

pub type RcModuleRegistry = ArcWithoutWeak<Mutex<ModuleRegistry>>;

pub enum ModuleError {
    /// The module did exist but could not be parsed.
    FailedToParse(String, bytecode_parser::ParserError),

    /// A given module did not exist.
    ModuleDoesNotExist(String),
}

pub struct ModuleRegistry {
    state: RcState,

    /// Mapping of the module names parsed thus far and their Module objects.
    parsed: HashMap<String, ObjectPointer>,
}

impl ModuleError {
    /// Returns a human friendly error message.
    pub fn message(&self) -> String {
        match *self {
            ModuleError::FailedToParse(ref path, ref error) => {
                format!("Failed to parse {}: {:?}", path, error)
            }
            ModuleError::ModuleDoesNotExist(ref path) => {
                format!("Module does not exist: {}", path)
            }
        }
    }
}

impl ModuleRegistry {
    pub fn with_rc(state: RcState) -> RcModuleRegistry {
        ArcWithoutWeak::new(Mutex::new(ModuleRegistry::new(state)))
    }

    pub fn new(state: RcState) -> Self {
        ModuleRegistry {
            state,
            parsed: HashMap::new(),
        }
    }

    /// Returns true if the given module has been parsed.
    #[cfg_attr(feature = "cargo-clippy", allow(ptr_arg))]
    pub fn contains(&self, name: &str) -> bool {
        self.parsed.contains_key(name)
    }

    /// Returns all parsed modules.
    pub fn parsed(&self) -> Vec<ObjectPointer> {
        self.parsed.values().copied().collect()
    }

    /// Obtains a parsed module by its name.
    pub fn get(&self, name: &str) -> Option<ObjectPointer> {
        self.parsed.get(name).copied()
    }

    /// Loads and defines a module with the given name and path.
    pub fn load(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<(ObjectPointer, bool), ModuleError> {
        if !self.parsed.contains_key(name) {
            let full_path = self.find_path(path)?;

            self.parse_module(name, &full_path)
                .map(|module| (module, true))
        } else {
            Ok((self.parsed[name], false))
        }
    }

    /// Returns the full path for a relative path.
    fn find_path(&self, path: &str) -> Result<String, ModuleError> {
        let mut input_path = PathBuf::from(path);

        if input_path.is_relative() {
            let mut found = false;

            for directory in &self.state.config.directories {
                let full_path = directory.join(path);

                if full_path.exists() {
                    input_path = full_path;
                    found = true;

                    break;
                }
            }

            if !found {
                return Err(ModuleError::ModuleDoesNotExist(path.to_string()));
            }
        }

        Ok(input_path.to_str().unwrap().to_string())
    }

    /// Parses a full file path pointing to a module.
    pub fn parse_module(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<ObjectPointer, ModuleError> {
        let code = bytecode_parser::parse_file(&self.state, path)
            .map_err(|err| ModuleError::FailedToParse(path.to_string(), err))?;

        Ok(self.define_module(name, path, code))
    }

    pub fn define_module(
        &mut self,
        name: &str,
        path: &str,
        code: CompiledCode,
    ) -> ObjectPointer {
        let name_obj = self.state.intern_string(name.to_string());
        let path_obj = self.state.intern_string(path.to_string());

        let module_val = object_value::module(ArcWithoutWeak::new(
            Module::new(name_obj, path_obj, code),
        ));

        let prototype = self.state.module_prototype;
        let module = self
            .state
            .permanent_allocator
            .lock()
            .allocate_with_prototype(module_val, prototype);

        self.parsed.insert(name.to_string(), module);

        module
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::vm::state::State;
    use std::env;
    use std::path::PathBuf;

    fn executable_path() -> PathBuf {
        env::current_exe().unwrap()
    }

    fn executable_path_string() -> String {
        executable_path().to_str().unwrap().to_string()
    }

    fn new_config() -> Config {
        let mut config = Config::new();

        config.add_directory(
            executable_path()
                .parent()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        );

        config
    }

    #[test]
    fn test_find_path_relative() {
        let state = State::with_rc(new_config(), &[]);
        let reg = ModuleRegistry::new(state);
        let look_for = executable_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let result = reg.find_path(&look_for);

        assert_eq!(result.ok().unwrap(), executable_path_string());
    }

    #[test]
    fn test_find_path_absolute() {
        let state = State::with_rc(new_config(), &[]);
        let reg = ModuleRegistry::new(state);
        let look_for = executable_path_string();
        let result = reg.find_path(&look_for);

        assert_eq!(result.ok().unwrap(), look_for);
    }
}
