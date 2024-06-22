//! Configuration for the compiler.
use crate::presenters::{JsonPresenter, Presenter, TextPresenter};
use crate::target::Target;
use std::collections::HashMap;
use std::env;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::thread::available_parallelism;
use std::time::SystemTime;
use types::module_name::ModuleName;

/// The extension to use for source files.
pub const SOURCE_EXT: &str = "inko";

/// The name of the module to compile if no explicit file/module is provided.
pub(crate) const MAIN_MODULE: &str = "main";

/// The name of the directory containing a project's source code.
pub(crate) const SOURCE: &str = "src";

/// The name of the directory containing third-party dependencies.
pub const DEP: &str = "dep";

/// The name of the directory containing a project's unit tests.
pub(crate) const TESTS: &str = "test";

/// The name of the module that runs tests.
const MAIN_TEST_MODULE: &str = "inko-tests";

/// The name of the directory to store build files in.
const BUILD: &str = "build";

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn data_directory() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library").join("Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local").join("share")))
    };

    base.map(|p| p.join("inko"))
}

pub fn local_runtimes_directory() -> Option<PathBuf> {
    // The Inko ABI isn't stable, so runtimes are scoped to the Inko version
    // they were compiled for.
    data_directory().map(|p| p.join("runtimes").join(env!("CARGO_PKG_VERSION")))
}

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

    /// The directory to store LLVM IR files in.
    pub(crate) llvm_ir: PathBuf,

    /// The directory to place executable files in.
    pub(crate) bin: PathBuf,

    /// The directory to write DOT files to.
    pub(crate) dot: PathBuf,

    /// The directory to store documentation files in.
    pub(crate) documentation: PathBuf,
}

impl BuildDirectories {
    pub(crate) fn new(config: &Config) -> BuildDirectories {
        let root = if config.target.is_native() {
            config.build.clone()
        } else {
            config.build.join(config.target.to_string())
        };

        let build = config
            .opt
            .directory_name()
            .map(|p| root.join(p))
            .unwrap_or_else(|| root.clone());

        let objects = build.join("objects");
        let llvm_ir = build.join("llvm");
        let dot = build.join("dot");
        let bin = build.clone();

        // The documentation isn't specific to the optimization level used, so
        // we always store it in the base build directory.
        let documentation = root.join("docs");

        BuildDirectories { build, objects, llvm_ir, bin, dot, documentation }
    }

    pub(crate) fn create(&self) -> Result<(), String> {
        self.create_build()
            .and_then(|_| create_directory(&self.objects))
            .and_then(|_| create_directory(&self.bin))
    }

    pub(crate) fn create_build(&self) -> Result<(), String> {
        create_directory(&self.build)
    }

    pub(crate) fn create_dot(&self) -> Result<(), String> {
        create_directory(&self.dot)
    }

    pub(crate) fn create_documentation(&self) -> Result<(), String> {
        create_directory(&self.documentation)
    }

    pub(crate) fn create_llvm(&self) -> Result<(), String> {
        create_directory(&self.llvm_ir)
    }
}

/// A type describing to what degree a program should be optimised.
#[derive(Copy, Clone)]
pub enum Opt {
    /// No optimisations are applied.
    None,

    /// A decent number of optimisations is applied, providing a good balance
    /// between runtime performance and compile times.
    Balanced,

    /// An aggressive number of optimisations is applied, favouring runtime
    /// performance over compile times.
    Aggressive,
}

impl Opt {
    pub(crate) fn directory_name(self) -> Option<&'static str> {
        match self {
            Opt::None => Some("none"),
            Opt::Balanced => None,
            Opt::Aggressive => Some("aggressive"),
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

/// A type describing which linker to use.
#[derive(Clone)]
pub enum Linker {
    /// Detect which linker to use.
    Detect,

    /// Always use the system linker.
    System,

    /// Always use LLD.
    Lld,

    /// Always use Mold.
    Mold,

    /// Always use Zig.
    Zig,

    /// Use a custom linker with any extra arguments.
    Custom(String),
}

impl Linker {
    pub fn parse(value: &str) -> Option<Linker> {
        match value {
            "system" => Some(Linker::System),
            "lld" => Some(Linker::Lld),
            "mold" => Some(Linker::Mold),
            "zig" => Some(Linker::Zig),
            _ if !value.is_empty() => Some(Linker::Custom(value.to_string())),
            _ => None,
        }
    }

    pub(crate) fn is_zig(&self) -> bool {
        matches!(self, Linker::Zig)
    }
}

/// A type for storing compiler configuration, such as the source directories to
/// search for modules.
pub struct Config {
    /// The directory containing the Inko's standard library.
    pub(crate) std: PathBuf,

    /// The path to the global runtime directory.
    pub runtime: PathBuf,

    /// The directory containing the project's source code.
    pub(crate) source: PathBuf,

    /// The directory containing the project's dependencies.
    pub dependencies: PathBuf,

    /// The directory containing the project's unit tests.
    pub tests: PathBuf,

    /// The directory to use for build output.
    pub build: PathBuf,

    /// A list of base source directories to search through.
    pub(crate) sources: Vec<PathBuf>,

    /// The path to save the executable at.
    pub output: Output,

    /// The optimisation mode to apply when compiling code.
    pub opt: Opt,

    /// The presenter to use for displaying diagnostics.
    pub(crate) presenter: Box<dyn Presenter + Sync>,

    /// The name of the initialization module to import into every module
    /// implicitly.
    pub(crate) init_module: ModuleName,

    /// The target to compile code for.
    pub(crate) target: Target,

    /// If MIR should be printed to DOT files.
    pub dot: bool,

    /// If LLVM IR should be verified as part of code generation.
    pub verify_llvm: bool,

    /// If LLVM IR should be written to disk.
    pub write_llvm: bool,

    /// If C libraries should be linked statically or not.
    pub static_linking: bool,

    /// The number of threads to use when performing work in parallel.
    pub threads: usize,

    /// The linker to use.
    pub linker: Linker,

    /// Extra arguments to pass to the linker.
    pub linker_arguments: Vec<String>,

    /// If incremental compilation is enabled or not.
    pub incremental: bool,

    /// The time at which the compiler executable was compiled.
    ///
    /// This is used to determine if incremental caches can be used or not. It's
    /// set here such that we can mock it when writing tests, should that be
    /// necessary, and to decouple the compiler logic from the command line as
    /// much as possible.
    pub compiled_at: SystemTime,

    /// Custom constant values to set at compile time.
    pub compile_time_variables: HashMap<(ModuleName, String), String>,
}

impl Config {
    pub(crate) fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::new());
        let std = PathBuf::from(env!("INKO_STD"));
        let compiled_at = env::current_exe()
            .and_then(|p| p.metadata())
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        Self {
            std,
            runtime: PathBuf::from(env!("INKO_RT")),
            source: cwd.join(SOURCE),
            tests: cwd.join(TESTS),
            build: cwd.join(BUILD),
            dependencies: cwd.join(DEP),
            sources: Vec::new(),
            presenter: Box::new(TextPresenter::with_colors()),
            init_module: ModuleName::std_init(),
            output: Output::Derive,
            target: Target::native(),
            opt: Opt::Balanced,
            dot: false,
            verify_llvm: false,
            write_llvm: false,
            static_linking: false,
            threads: available_parallelism().map(|v| v.get()).unwrap_or(1),
            linker: Linker::Detect,
            linker_arguments: Vec::new(),
            incremental: true,
            compiled_at,
            compile_time_variables: HashMap::new(),
        }
    }

    fn add_default_source_directories(&mut self) {
        if self.std.is_dir() {
            self.sources.push(self.std.clone());
        }
    }

    pub fn add_source_directory(&mut self, path: PathBuf) {
        self.sources.push(path.canonicalize().unwrap_or(path));
    }

    pub fn set_presenter(&mut self, format: &str) -> Result<(), String> {
        self.presenter = match format {
            "text" => Box::new(TextPresenter::with_colors()),
            "plain" => Box::new(TextPresenter::without_colors()),
            "json" => Box::new(JsonPresenter::new()),
            _ => return Err(format!("The presenter {:?} is invalid", format)),
        };

        Ok(())
    }

    pub fn set_target(&mut self, name: &str) -> Result<(), String> {
        if let Some(val) = Target::parse(name) {
            self.target = val;
            Ok(())
        } else {
            Err(format!("The target '{}' isn't supported", name))
        }
    }

    pub fn set_opt(&mut self, name: &str) -> Result<(), String> {
        self.opt = match name {
            "none" => Opt::None,
            "balanced" => Opt::Balanced,
            "aggressive" => Opt::Aggressive,
            _ => {
                return Err(format!(
                    "The optimisation level '{}' isn't supported",
                    name
                ))
            }
        };

        Ok(())
    }

    pub(crate) fn main_source_module(&self) -> PathBuf {
        let mut main_file = self.source.join(MAIN_MODULE);

        main_file.set_extension(SOURCE_EXT);
        main_file
    }

    pub fn main_test_module(&self) -> PathBuf {
        let mut main_file = self.build.join(MAIN_TEST_MODULE);

        main_file.set_extension(SOURCE_EXT);
        main_file
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut cfg = Config::new();

        cfg.add_default_source_directories();
        cfg
    }
}
