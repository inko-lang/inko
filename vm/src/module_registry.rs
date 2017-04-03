//! Parsing and caching of bytecode modules.
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;

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
    ///
    /// If a module has already been parsed for the given path it's returned
    /// directly, otherwise this method will attempt to parse it.
    pub fn get_or_set(&mut self, path: &String) -> Result<&Module, ModuleError> {
        if !self.parsed.contains_key(path) {
            self.parse_module(path)
        } else {
            Ok(self.parsed.get(path).unwrap())
        }
    }

    /// Parses a module.
    fn parse_module(&mut self, path: &String) -> Result<&Module, ModuleError> {
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
                return Err(ModuleError::ModuleDoesNotExist(path.clone()));
            }
        }

        let parse_path = input_path.to_str().unwrap();

        self.parse_path(parse_path)
    }

    /// Parses a full file path pointing to a module.
    pub fn parse_path(&mut self, path: &str) -> Result<&Module, ModuleError> {
        let code = bytecode_parser::parse_file(&self.state, path)
            .map_err(|err| ModuleError::FailedToParse(path.to_string(), err))?;

        self.add_module(path, Module::new(code));

        Ok(self.parsed.get(path).unwrap())
    }

    pub fn add_module(&mut self, path: &str, module: Module) {
        self.parsed.insert(path.to_string(), module);
    }
}
