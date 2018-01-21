//! Parsing and caching of bytecode modules.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use bytecode_parser;
use module::Module;
use vm::state::RcState;

pub type RcModuleRegistry = Arc<RwLock<ModuleRegistry>>;

pub enum ModuleError {
    /// The module did exist but could not be parsed.
    FailedToParse(String, bytecode_parser::ParserError),

    /// A given module did not exist.
    ModuleDoesNotExist(String),
}

pub struct ModuleRegistry {
    state: RcState,
    parsed: HashMap<String, Module>,
}

pub struct LookupResult<'a> {
    pub module: &'a Module,

    /// Set to true when the module was parsed for the first time.
    pub parsed: bool,
}

impl ModuleError {
    /// Returns a human friendly error message.
    pub fn message(&self) -> String {
        match self {
            &ModuleError::FailedToParse(ref path, ref error) => {
                format!("Failed to parse {}: {:?}", path, error)
            }
            &ModuleError::ModuleDoesNotExist(ref path) => {
                format!("Module does not exist: {}", path)
            }
        }
    }
}

impl<'a> LookupResult<'a> {
    pub fn new(module: &'a Module, parsed: bool) -> Self {
        LookupResult {
            module: module,
            parsed: parsed,
        }
    }
}

impl ModuleRegistry {
    pub fn with_rc(state: RcState) -> RcModuleRegistry {
        Arc::new(RwLock::new(ModuleRegistry::new(state)))
    }

    pub fn new(state: RcState) -> Self {
        ModuleRegistry {
            state: state,
            parsed: HashMap::new(),
        }
    }

    /// Returns true if the given module has been parsed.
    pub fn contains_path(&self, path: &String) -> bool {
        self.parsed.contains_key(path)
    }

    /// Gets or parses a bytecode file for the given path.
    pub fn get_or_set(
        &mut self,
        path: &str,
    ) -> Result<LookupResult, ModuleError> {
        let full_path = self.find_path(path)?;

        if !self.parsed.contains_key(&full_path) {
            self.parse_module(&full_path)
                .map(|module| LookupResult::new(module, true))
        } else {
            Ok(LookupResult::new(
                self.parsed.get(&full_path).unwrap(),
                false,
            ))
        }
    }

    /// Returns the full path for a relative path.
    fn find_path(&self, path: &str) -> Result<String, ModuleError> {
        let mut input_path = PathBuf::from(path);

        if input_path.is_relative() {
            let mut found = false;

            for directory in self.state.config.directories.iter() {
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
    pub fn parse_module(&mut self, path: &str) -> Result<&Module, ModuleError> {
        let code = bytecode_parser::parse_file(&self.state, path)
            .map_err(|err| ModuleError::FailedToParse(path.to_string(), err))?;

        self.add_module(path, Module::new(code));

        Ok(self.parsed.get(path).unwrap())
    }

    pub fn add_module(&mut self, path: &str, module: Module) {
        self.parsed.insert(path.to_string(), module);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use vm::state::State;

    fn new_config() -> Config {
        let mut config = Config::new();

        config.add_directory("/bin".to_string());

        config
    }

    #[test]
    fn test_find_path_relative() {
        let state = State::new(new_config());
        let reg = ModuleRegistry::new(state);
        let result = reg.find_path("ls");

        assert_eq!(result.ok().unwrap(), "/bin/ls".to_string());
    }

    #[test]
    fn test_find_path_absolute() {
        let state = State::new(new_config());
        let reg = ModuleRegistry::new(state);
        let result = reg.find_path("/bin/ls");

        assert_eq!(result.ok().unwrap(), "/bin/ls".to_string());
    }
}
