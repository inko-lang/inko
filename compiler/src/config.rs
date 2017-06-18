use std::path::PathBuf;

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
}

impl Config {
    pub fn new(target: PathBuf) -> Config {
        Config {
            source_directories: Vec::new(),
            mode: Mode::Debug,
            target: target,
        }
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
