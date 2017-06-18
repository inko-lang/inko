use xdg::BaseDirectories;
use std::path::PathBuf;

const PROGRAM_NAME: &'static str = "inkoc";
const BYTECODE_DIR: &'static str = "bytecode";

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

    /// Returns the file extension to use for source files.
    pub fn source_extension(&self) -> &'static str {
        ".inko"
    }

    /// Returns the file extension to use for bytecode files.
    pub fn bytecode_extension(&self) -> &'static str {
        ".inkoc"
    }

    /// Returns the separator used for module/method/constant lookups.
    pub fn lookup_separator(&self) -> &'static str {
        "::"
    }

    /// The name of the constant to use as the receiver for raw instructions.
    pub fn raw_instruction_receiver(&self) -> &'static str {
        "__INKOC"
    }

    /// The name of the attribute to store the prototype for a class' instance
    /// in.
    pub fn instance_prototype(&self) -> String {
        "__proto".to_string()
    }

    pub fn self_variable(&self) -> String {
        "self".to_string()
    }
}
