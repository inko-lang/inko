//! Parsing and caching of bytecode files.
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;

use bytecode_parser;
use compiled_code::RcCompiledCode;
use vm::state::RcState;

pub type RcFileRegistry = Arc<RwLock<FileRegistry>>;

pub enum FileError {
    /// The bytecode file did exist but could not be parsed.
    FailedToParse(String, bytecode_parser::ParserError),

    /// A given bytecode file did not exist.
    FileDoesNotExist(String),
}

pub struct FileRegistry {
    state: RcState,
    parsed: HashMap<String, RcCompiledCode>,
}

impl FileError {
    /// Returns a human friendly error message.
    pub fn message(&self) -> String {
        match self {
            &FileError::FailedToParse(ref path, ref error) => {
                format!("Failed to parse {}: {:?}", path, error)
            }
            &FileError::FileDoesNotExist(ref path) => {
                format!("Bytecode file does not exist: {}", path)
            }
        }
    }
}

impl FileRegistry {
    pub fn with_rc(state: RcState) -> RcFileRegistry {
        Arc::new(RwLock::new(FileRegistry::new(state)))
    }

    pub fn new(state: RcState) -> Self {
        FileRegistry {
            state: state,
            parsed: HashMap::new(),
        }
    }

    /// Returns true if the given file has been parsed.
    pub fn contains_path(&self, path: &String) -> bool {
        self.parsed.contains_key(path)
    }

    /// Gets or parses a bytecode file for the given path.
    ///
    /// If a bytecode file has already been parsed for the given path it's
    /// returned directly, otherwise this method will attempt to parse it.
    pub fn get_or_set(&mut self,
                      path: &String)
                      -> Result<RcCompiledCode, FileError> {
        if !self.parsed.contains_key(path) {
            self.parse_file(path)?;
        }

        Ok(self.parsed.get(path).unwrap().clone())
    }

    /// Parses a bytecode file.
    fn parse_file(&mut self, path: &String) -> Result<RcCompiledCode, FileError> {
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
                return Err(FileError::FileDoesNotExist(path.clone()));
            }
        }

        let parse_path = input_path.to_str().unwrap();

        let code = bytecode_parser::parse_file(&self.state, parse_path)
            .map_err(|err| FileError::FailedToParse(path.clone(), err))?;

        self.parsed.insert(path.clone(), code.clone());

        Ok(code)
    }
}
