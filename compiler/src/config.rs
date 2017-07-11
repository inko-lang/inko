use xdg::BaseDirectories;
use std::path::PathBuf;

pub const PROGRAM_NAME: &'static str = "inkoc";

/// The name of the directory to store bytecode files in.
pub const BYTECODE_DIR: &'static str = "bytecode";

/// The file extension of bytecode files.
pub const BYTECODE_EXT: &'static str = ".inkoc";

/// The file extension of source files.
pub const SOURCE_EXT: &'static str = ".inko";

/// The name of the bootstrap module.
pub const BOOTSTRAP_FILE: &'static str = "bootstrap";

pub const OBJECT_CONST: &'static str = "Object";
pub const TRAIT_CONST: &'static str = "Trait";

/// The name of the constant to use as the receiver for raw instructions.
pub const RAW_INSTRUCTION_RECEIVER: &'static str = "__INKOC";

pub enum Mode {
    Debug,
    Release,
}

pub struct Config {
    /// The directories to search for source files.
    pub source_directories: Vec<PathBuf>,

    /// The mode to use for compiling code.
    pub mode: Mode,

    /// The directory to store bytecode files in.
    pub target: PathBuf,

    base_directory: BaseDirectories,
}

impl Config {
    pub fn new() -> Config {
        let base_dir = BaseDirectories::with_prefix(PROGRAM_NAME).unwrap();

        Config {
            source_directories: Vec::new(),
            mode: Mode::Debug,
            target: base_dir.get_cache_home().join(BYTECODE_DIR),
            base_directory: base_dir,
        }
    }

    pub fn create_directories(&self) {
        self.base_directory
            .create_cache_directory(BYTECODE_DIR)
            .unwrap();
    }

    pub fn set_target(&mut self, path: PathBuf) {
        self.target = path;
    }

    pub fn set_release_mode(&mut self) {
        self.mode = Mode::Release;
    }

    pub fn add_source_directory(&mut self, dir: String) {
        self.source_directories.push(PathBuf::from(dir));
    }

    pub fn new_message(&self) -> String {
        "new".to_string()
    }

    pub fn define_required_method_message(&self) -> String {
        "define_required_method".to_string()
    }

    pub fn call_message(&self) -> String {
        "call".to_string()
    }

    pub fn self_variable(&self) -> String {
        "self".to_string()
    }

    pub fn load_module_message(&self) -> String {
        "load_module".to_string()
    }

    pub fn symbol_message(&self) -> String {
        "symbol".to_string()
    }

    pub fn define_module_message(&self) -> String {
        "define_module".to_string()
    }
}
