//! Configuration for the compiler.
use crate::presenters::{JSONPresenter, Presenter, TextPresenter};
use crate::source_paths::SourcePaths;
use crate::target::Target;
use std::env;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use types::module_name::ModuleName;

/// The extension to use for source files.
pub(crate) const SOURCE_EXT: &str = "inko";

/// The name of the module to compile if no explicit file/module is provided.
pub(crate) const MAIN_MODULE: &str = "main";

/// The name of the directory containing a project's source code.
pub(crate) const SOURCE: &str = "src";

/// The name of the directory containing third-party dependencies.
pub const DEP: &str = "dep";

/// The name of the directory containing a project's unit tests.
const TESTS: &str = "test";

/// The name of the directory to store build files in.
const BUILD: &str = "build";

fn create_directory(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        return Ok(());
    }

    create_dir_all(path)
        .map_err(|err| format!("Failed to create {}: {}", path.display(), err))
}

/// A type storing the various build directories to use.
pub(crate) struct BuildDirectories {
    /// The base build directory.
    pub(crate) build: PathBuf,

    /// The directory to store object files in.
    pub(crate) objects: PathBuf,

    /// The directory to place executable files in.
    pub(crate) bin: PathBuf,

    /// The directory to write DOT files to.
    pub(crate) dot: PathBuf,
}

impl BuildDirectories {
    pub(crate) fn new(config: &Config) -> BuildDirectories {
        let build = config.build.join(config.mode.directory_name());
        let objects = build.join("objects");
        let dot = build.join("dot");
        let bin = build.clone();

        BuildDirectories { build, objects, bin, dot }
    }

    pub(crate) fn create(&self) -> Result<(), String> {
        create_directory(&self.build)
            .and_then(|_| create_directory(&self.objects))
            .and_then(|_| create_directory(&self.bin))
    }

    pub(crate) fn create_dot(&self) -> Result<(), String> {
        create_directory(&self.dot)
    }
}

/// A type describing to what degree a program should be optimised.
#[derive(Clone, Copy)]
pub enum Mode {
    /// A mode suitable for development and debugging.
    ///
    /// This mode favours fast compile times over runtime performance. For
    /// releases/deployments you should use the dedicated release mode.
    Debug,

    /// A mode suitable for releases.
    ///
    /// In this mode a reasonable number of optimisations is enabled, such that
    /// there's a healthy balance between runtime performance and compile times.
    Release,
}

impl Mode {
    pub(crate) fn directory_name(self) -> &'static str {
        match self {
            Mode::Debug => "debug",
            Mode::Release => "release",
        }
    }
}

/// A type describing where to write the executable to.
pub enum Output {
    /// Derive the output path from the main module, and place it in the default
    /// output directory.
    Derive,

    /// Write the executable to the default output directory, but using the
    /// given name.
    File(String),

    /// Write the executable to the given path.
    Path(PathBuf),
}

/// A type for storing compiler configuration, such as the source directories to
/// search for modules.
pub struct Config {
    /// The directory containing the Inko's standard library.
    pub(crate) std: PathBuf,

    /// The directory containing runtime library files to link to the generated
    /// code.
    pub runtime: PathBuf,

    /// The directory containing the project's source code.
    pub(crate) source: PathBuf,

    /// The directory containing the project's dependencies.
    pub dependencies: PathBuf,

    /// The directory containing the project's unit tests.
    pub tests: PathBuf,

    /// The directory to use for build output.
    pub build: PathBuf,

    /// A list of directories to search for Inko source code, including
    /// third-party dependencies.
    pub sources: SourcePaths,

    /// The path to save the executable at.
    pub output: Output,

    /// The optimisation mode to apply when compiling code.
    pub mode: Mode,

    /// The presenter to use for displaying diagnostics.
    pub(crate) presenter: Box<dyn Presenter>,

    /// Modules to implicitly import and process.
    pub(crate) implicit_imports: Vec<ModuleName>,

    /// The target to compile code for.
    pub(crate) target: Target,

    /// If MIR should be printed to DOT files.
    pub dot: bool,
}

impl Config {
    pub(crate) fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::new());
        let std = PathBuf::from(env!("INKO_STD"));

        Self {
            std,
            runtime: PathBuf::from(env!("INKO_RT")),
            source: cwd.join(SOURCE),
            tests: cwd.join(TESTS),
            build: cwd.join(BUILD),
            dependencies: cwd.join(DEP).join(SOURCE),
            sources: SourcePaths::new(),
            presenter: Box::new(TextPresenter::with_colors()),
            implicit_imports: vec![],
            output: Output::Derive,
            target: Target::native(),
            mode: Mode::Debug,
            dot: false,
        }
    }

    fn add_default_source_directories(&mut self) {
        if self.std.is_dir() {
            self.sources.add(self.std.clone());
        }

        if self.source.is_dir() && self.source != self.std {
            self.sources.add(self.source.clone());
        }

        if self.dependencies.is_dir() {
            self.sources.add(self.dependencies.clone());
        }
    }

    fn add_default_implicit_imports(&mut self) {
        self.implicit_imports.push(ModuleName::std_init());
    }

    pub fn set_presenter(&mut self, format: &str) -> Result<(), String> {
        self.presenter = match format {
            "text" => Box::new(TextPresenter::with_colors()),
            "plain" => Box::new(TextPresenter::without_colors()),
            "json" => Box::new(JSONPresenter::new()),
            _ => return Err(format!("The presenter {:?} is invalid", format)),
        };

        Ok(())
    }

    pub fn set_target(&mut self, name: &str) -> Result<(), String> {
        if let Some(val) = Target::from_str(name) {
            self.target = val;
            Ok(())
        } else {
            Err(format!("The target '{}' isn't supported", name))
        }
    }

    pub(crate) fn main_source_module(&self) -> PathBuf {
        let mut main_file = self.source.join(MAIN_MODULE);

        main_file.set_extension(SOURCE_EXT);
        main_file
    }

    pub fn main_test_module(&self) -> PathBuf {
        let mut main_file = self.tests.join(MAIN_MODULE);

        main_file.set_extension(SOURCE_EXT);
        main_file
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut cfg = Config::new();

        cfg.add_default_source_directories();
        cfg.add_default_implicit_imports();
        cfg
    }
}
