//! Configuration for the compiler.
use crate::presenters::{JSONPresenter, Presenter, TextPresenter};
use crate::source_paths::SourcePaths;
use std::env;
use std::path::PathBuf;
use types::module_name::ModuleName;

/// The extension to use for source files.
pub const SOURCE_EXT: &str = "inko";

/// The extension to use for bytecode files.
pub const IMAGE_EXT: &str = "ibi";

/// The name of the module to compile if no explicit file/module is provided.
pub(crate) const MAIN_MODULE: &str = "main";

/// The name of the directory containing a project's source code.
pub(crate) const SOURCE: &str = "src";

/// The name of the directory containing a project's unit tests.
const TESTS: &str = "test";

/// The name of the directory to store build files in.
const BUILD: &str = "build";

/// A type for storing compiler configuration, such as the source directories to
/// search for modules.
pub struct Config {
    /// The directory containing the Inko's standard library.
    pub(crate) libstd: PathBuf,

    /// The directory containing the project's source code.
    pub(crate) source: PathBuf,

    /// The directory containing the project's unit tests.
    pub tests: PathBuf,

    /// The directory to use for build output.
    pub build: PathBuf,

    /// A list of directories to search for Inko source code, including
    /// third-party dependencies.
    pub sources: SourcePaths,

    /// The presenter to use for displaying diagnostics.
    pub(crate) presenter: Box<dyn Presenter>,

    /// Modules to implicitly import and process.
    pub(crate) implicit_imports: Vec<ModuleName>,

    /// The file to write a compiled bytecode file to.
    pub output: Option<PathBuf>,
}

impl Config {
    pub(crate) fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::new());
        let libstd = option_env!("INKO_LIBSTD")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                // To ease the development process, we default to the standard
                // library directory in the Git repository. This way you don't
                // need to set any environment variables during development.
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join("libstd")
                    .join(SOURCE)
            });

        Self {
            libstd,
            source: cwd.join(SOURCE),
            tests: cwd.join(TESTS),
            build: cwd.join(BUILD),
            sources: SourcePaths::new(),
            presenter: Box::new(TextPresenter::with_colors()),
            implicit_imports: vec![],
            output: None,
        }
    }

    fn add_default_source_directories(&mut self) {
        if self.libstd.is_dir() {
            self.sources.add(self.libstd.clone());
        }

        if self.source.is_dir() && self.source != self.libstd {
            self.sources.add(self.source.clone());
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
