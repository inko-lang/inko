use crate::config::BuildDirectories;
use crate::config::{Config, Opt, SOURCE, SOURCE_EXT, TESTS};
use crate::docs::{
    Config as DocsConfig, DefineDocumentation, GenerateDocumentation,
};
use crate::hir;
use crate::linker::link;
use crate::llvm;
use crate::mir::inline::InlineMethod;
use crate::mir::passes as mir;
use crate::mir::printer::to_dot;
use crate::mir::specialize::Specialize;
use crate::mir::Mir;
use crate::modules_parser::{ModulesParser, ParsedModule};
use crate::pkg::manifest::Manifest;
use crate::pkg::sync::sync_if_needed;
use crate::pkg::version::Version;
use crate::state::State;
use crate::symbol_names::SymbolNames;
use crate::type_check::define_types::{
    check_recursive_types, CheckTraitImplementations, CheckTraitRequirements,
    CheckTypeParameters, DefineConstructors, DefineFields,
    DefineTraitRequirements, DefineTypeParameterRequirements,
    DefineTypeParameters, DefineTypes, ImplementTraits, InsertPrelude,
};
use crate::type_check::expressions::{define_constants, Expressions};
use crate::type_check::imports::{
    check_unused_imports, CollectExternImports, DefineImportedTypes,
};
use crate::type_check::methods::{
    CheckMainMethod, DefineMethods, DefineModuleMethodNames,
    ImplementTraitMethods,
};
use blake3::hash;
use std::ffi::OsStr;
use std::fs::{create_dir_all, write};
use std::path::MAIN_SEPARATOR_STR;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use types::module_name::{ModuleName, SEPARATOR};

fn measure<R, F: FnOnce() -> R>(time: &mut Duration, func: F) -> R {
    let start = Instant::now();
    let res = func();

    *time = start.elapsed();
    res
}

fn module_name_from_path(config: &Config, file: &Path) -> ModuleName {
    file.strip_prefix(&config.source)
        .ok()
        .or_else(|| file.strip_prefix(&config.tests).ok())
        .or_else(|| {
            // This allows us to check e.g. `./std/src/std/string.inko`
            // while the current working directory is `.`. This is useful
            // when e.g. checking files using a text editor, as they would
            // likely have the working directory set to `.` and not
            // `./std`.
            let mut components = file.components();

            if components
                .any(|c| c.as_os_str() == SOURCE || c.as_os_str() == TESTS)
            {
                Some(components.as_path())
            } else {
                None
            }
        })
        .map(ModuleName::from_relative_path)
        .unwrap_or_else(ModuleName::main)
}

pub(crate) fn module_debug_path(module: &ModuleName) -> PathBuf {
    let name = module.as_str();

    // When splitting modules we include the type shapes in the name in order to
    // prevent naming conflicts. This can result in very long file names,
    // possibly longer than the file system allows. To prevent that from
    // becoming a problem, we hash the shapes if they're present.
    if let Some((head, tail)) = name.split_once("<closure>") {
        PathBuf::from(format!(
            "{}<closure>{}",
            head.replace(SEPARATOR, MAIN_SEPARATOR_STR),
            hash(tail.as_bytes()),
        ))
    } else if let Some((head, tail)) = name.split_once('#') {
        PathBuf::from(format!(
            "{}#{}",
            head.replace(SEPARATOR, MAIN_SEPARATOR_STR),
            hash(tail.as_bytes()),
        ))
    } else {
        PathBuf::from(name.replace(SEPARATOR, MAIN_SEPARATOR_STR))
    }
}

pub(crate) fn all_source_modules(
    config: &Config,
    include_tests: bool,
) -> Result<Vec<(ModuleName, PathBuf)>, String> {
    let mut modules = Vec::new();
    let mut paths = Vec::new();
    let src_ext = OsStr::new(SOURCE_EXT);
    let source = &config.source;
    let tests = &config.tests;

    if source.is_dir() {
        paths.push(source.clone());
    }

    if include_tests && tests.is_dir() {
        paths.push(tests.clone());
    }

    while let Some(path) = paths.pop() {
        let iter = path.read_dir().map_err(|e| {
            format!("failed to read directory {:?}: {}", path, e)
        })?;

        for entry in iter {
            let path = entry
                .map_err(|e| {
                    format!("failed to read the contents of {:?}: {}", path, e)
                })?
                .path();

            if path.is_dir() {
                paths.push(path);
            } else if path.is_file() && path.extension() == Some(src_ext) {
                modules.push((module_name_from_path(config, &path), path));
            }
        }
    }

    Ok(modules)
}

fn format_timing(duration: Duration, total: Option<Duration>) -> String {
    let base = if duration.as_secs() >= 1 {
        format!("{:.2} sec", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{} msec", duration.as_millis())
    } else if duration.as_micros() >= 1 {
        format!("{} µsec", duration.as_micros())
    } else {
        format!("{} nsec", duration.as_nanos())
    };

    if let Some(total) = total {
        if total.as_nanos() == 0 {
            return base;
        }

        let percent =
            ((duration.as_secs_f64() / total.as_secs_f64()) * 100.0) as u64;

        let line = format!("{} ({}%)", base, percent);

        if percent >= 20 {
            format!("\x1b[31m{}\x1b[0m", line)
        } else {
            line
        }
    } else {
        base
    }
}

struct OptimizationTimings {
    prepare: Duration,
    inline: Duration,
    remove_methods: Duration,
    method_local: Duration,
    total: Duration,
}

struct Timings {
    ast: Duration,
    hir: Duration,
    type_check: Duration,
    mir: Duration,
    specialize_mir: Duration,
    optimize: OptimizationTimings,
    llvm: Duration,
    llvm_modules: Vec<(ModuleName, Duration)>,
    link: Duration,
    total: Duration,
}

impl Timings {
    fn new() -> Timings {
        Timings {
            ast: Duration::from_secs(0),
            hir: Duration::from_secs(0),
            type_check: Duration::from_secs(0),
            mir: Duration::from_secs(0),
            specialize_mir: Duration::from_secs(0),
            optimize: OptimizationTimings {
                prepare: Duration::from_secs(0),
                inline: Duration::from_secs(0),
                remove_methods: Duration::from_secs(0),
                method_local: Duration::from_secs(0),
                total: Duration::from_secs(0),
            },
            llvm: Duration::from_secs(0),
            llvm_modules: Vec::new(),
            link: Duration::from_secs(0),
            total: Duration::from_secs(0),
        }
    }
}

pub enum CompileError {
    /// The input program is invalid (e.g. there are type errors).
    Invalid,

    /// The compiler produced an internal error and couldn't proceed.
    Internal(String),
}

impl From<String> for CompileError {
    fn from(value: String) -> CompileError {
        CompileError::Internal(value)
    }
}

pub struct Compiler {
    state: State,
    timings: Timings,
}

impl Compiler {
    pub fn new(config: Config) -> Self {
        Self { state: State::new(config), timings: Timings::new() }
    }

    pub fn check(&mut self, file: Option<PathBuf>) -> Result<(), CompileError> {
        self.prepare()?;

        let start = Instant::now();

        // When checking a project we want to fall back to checking _all_ files
        // including tests, not just the main module.
        //
        // We don't define the main module, as this allows for type-checking
        // libraries, which won't provide such a module.
        let input = if let Some(file) = file {
            let file = file.canonicalize().unwrap_or(file);

            vec![(module_name_from_path(&self.state.config, &file), file)]
        } else {
            all_source_modules(&self.state.config, true)
                .map_err(CompileError::Internal)?
        };

        let ast = self.parse(input);
        let mut hir = self.compile_hir(ast)?;

        self.check_types(&mut hir)?;

        let res = self.compile_mir(hir).map(|_| ());

        self.timings.total = start.elapsed();
        res
    }

    pub fn build(&mut self, file: PathBuf) -> Result<PathBuf, CompileError> {
        self.prepare()?;

        let start = Instant::now();
        let file = file.canonicalize().unwrap_or(file);
        let main_mod = module_name_from_path(&self.state.config, &file);

        self.state.db.set_main_module(main_mod.clone());

        let ast = self.parse(vec![(main_mod, file.clone())]);
        let mut hir = self.compile_hir(ast)?;

        self.check_types(&mut hir)?;

        let mut mir = self.compile_mir(hir)?;

        // Type specialization _must_ be done before optimizations and lowering
        // MIR to LLVM, otherwise we may generate incorrect code.
        self.specialize_mir(&mut mir);

        // At this point we can get rid of various data structures stored in the
        // type database. This must be done _after_ specialization.
        self.state.db.compact();

        // Splitting is done _after_ specialization, since specialization
        // introduces new types and methods.
        mir.split_modules(&mut self.state);

        // Symbol names are needed to ensure certain passes can operate on data
        // in a stable order, which in turn is needed to ensure incremental
        // caches aren't flushed unnecessarily.
        //
        // Creating the symbol names and sorting MIR _must_ be done _after_
        // splitting modules but _before_ optimizing MIR, such that the
        // optimization passes can perform their work in a stable order.
        let symbols = SymbolNames::new(&self.state.db, &mir);

        mir.sort(&self.state.db, &symbols);
        self.optimise_mir(&mut mir);

        if self.state.config.verify {
            mir.verify(&self.state.db, &symbols)?;
        }

        let dirs = BuildDirectories::new(&self.state.config);

        dirs.create().map_err(CompileError::Internal)?;

        if self.state.config.dot {
            self.write_dot(&dirs, &mir)?;
        }

        let res = self.compile_machine_code(&dirs, mir, &symbols, file);

        self.timings.total = start.elapsed();
        res
    }

    pub fn document(&mut self, config: DocsConfig) -> Result<(), CompileError> {
        self.prepare()?;

        // When generating documentation we don't include the unit tests.
        let input = all_source_modules(&self.state.config, false)
            .map_err(CompileError::Internal)?;
        let ast = ModulesParser::with_documentation_comments(&mut self.state)
            .run(input);
        let mut hir = self.compile_hir(ast)?;

        self.check_types(&mut hir)?;

        // The MIR passes take ownership of the HIR, so we run this first.
        DefineDocumentation::run_all(&mut self.state, &mut hir);
        self.compile_mir(hir).map(|_| ())?;

        let dirs = BuildDirectories::new(&self.state.config);

        dirs.create()
            .and_then(|_| dirs.create_documentation())
            .and_then(|_| {
                GenerateDocumentation::run_all(&self.state, &dirs, &config)
            })
            .map_err(CompileError::Internal)
    }

    pub fn print_diagnostics(&self) {
        self.state.config.presenter.present(&self.state.diagnostics);
    }

    pub fn print_timings(&self) {
        let total = self.timings.total;

        // Diagnostics go to STDERR, so we print to STDOUT here, allowing users
        // to still get the diagnostics without these timings messing things up.
        println!(
            "\
Frontend:
  Parse       {ast}
  AST to HIR  {hir}
  Type check  {type_check}
  HIR to MIR  {mir}
  Specialize  {specialize}

Optimizations:
  Prepare                {opt_prep}
  Inline                 {opt_inline}
  Remove unused methods  {opt_unused_methods}
  Method local           {opt_method_local}
  Total                  {opt_total}

Backend:
  LLVM    {llvm}
  Linker  {link}

Total: {total}\
            ",
            ast = format_timing(self.timings.ast, Some(total)),
            hir = format_timing(self.timings.hir, Some(total)),
            type_check = format_timing(self.timings.type_check, Some(total)),
            mir = format_timing(self.timings.mir, Some(total)),
            specialize =
                format_timing(self.timings.specialize_mir, Some(total)),
            opt_prep =
                format_timing(self.timings.optimize.prepare, Some(total)),
            opt_inline =
                format_timing(self.timings.optimize.inline, Some(total)),
            opt_unused_methods = format_timing(
                self.timings.optimize.remove_methods,
                Some(total)
            ),
            opt_method_local =
                format_timing(self.timings.optimize.method_local, Some(total)),
            opt_total = format_timing(self.timings.optimize.total, Some(total)),
            llvm = format_timing(self.timings.llvm, Some(total)),
            link = format_timing(self.timings.link, Some(total)),
            total = format_timing(self.timings.total, None),
        );
    }

    pub fn print_full_timings(&self) {
        self.print_timings();

        let width = self
            .timings
            .llvm_modules
            .iter()
            .map(|v| v.0.as_str().len())
            .max()
            .unwrap_or(0);

        println!(
            "
LLVM module timings:

\x1b[1m{:width$}\x1b[0m    \x1b[1mTime\x1b[0m\
            ",
            "Module",
            width = width
        );

        for (name, dur) in &self.timings.llvm_modules {
            println!(
                "{:width$}    {}",
                name.as_str(),
                format_timing(*dur, Some(self.timings.llvm)),
                width = width
            );
        }
    }

    pub fn create_build_directory(&self) -> Result<(), String> {
        BuildDirectories::new(&self.state.config).create_build()
    }

    pub fn into_config(self) -> Config {
        self.state.config
    }

    fn compile_mir(
        &mut self,
        modules: Vec<hir::Module>,
    ) -> Result<Mir, CompileError> {
        let start = Instant::now();
        let mut mir = Mir::new();
        let state = &mut self.state;
        let ok = if mir::DefineConstants::run_all(state, &mut mir, &modules) {
            mir::define_default_compile_time_variables(state);
            mir::apply_compile_time_variables(state, &mut mir)
                .map_err(CompileError::Internal)?;
            mir::LowerToMir::run_all(state, &mut mir, modules)
        } else {
            false
        };

        self.timings.mir = start.elapsed();

        if ok {
            Ok(mir)
        } else {
            Err(CompileError::Invalid)
        }
    }

    fn parse(
        &mut self,
        input: Vec<(ModuleName, PathBuf)>,
    ) -> Vec<ParsedModule> {
        let start = Instant::now();
        let res = ModulesParser::new(&mut self.state).run(input);

        self.timings.ast = start.elapsed();
        res
    }

    fn compile_hir(
        &mut self,
        modules: Vec<ParsedModule>,
    ) -> Result<Vec<hir::Module>, CompileError> {
        let start = Instant::now();
        let hir = hir::LowerToHir::run_all(&mut self.state, modules);

        self.timings.hir = start.elapsed();

        // Errors produced at this state are likely to result in us not being
        // able to compile the program properly (e.g. imported modules don't
        // exist), so we bail out right away.
        if self.state.diagnostics.has_errors() {
            Err(CompileError::Invalid)
        } else {
            Ok(hir)
        }
    }

    fn check_types(
        &mut self,
        modules: &mut Vec<hir::Module>,
    ) -> Result<(), CompileError> {
        let state = &mut self.state;
        let start = Instant::now();
        let res = DefineTypes::run_all(state, modules)
            && CollectExternImports::run_all(state, modules)
            && DefineModuleMethodNames::run_all(state, modules)
            && DefineImportedTypes::run_all(state, modules)
            && InsertPrelude::run_all(state, modules)
            && DefineTypeParameters::run_all(state, modules)
            && DefineTypeParameterRequirements::run_all(state, modules)
            && DefineTraitRequirements::run_all(state, modules)
            && CheckTraitRequirements::run_all(state, modules)
            && ImplementTraits::run_all(state, modules)
            && CheckTraitImplementations::run_all(state, modules)
            && CheckTypeParameters::run_all(state, modules)
            && DefineConstructors::run_all(state, modules)
            && DefineFields::run_all(state, modules)
            && check_recursive_types(state, modules)
            && DefineMethods::run_all(state, modules)
            && CheckMainMethod::run(state)
            && ImplementTraitMethods::run_all(state, modules)
            && define_constants(state, modules)
            && Expressions::run_all(state, modules)
            && check_unused_imports(state, modules);

        self.timings.type_check = start.elapsed();

        if res {
            Ok(())
        } else {
            Err(CompileError::Invalid)
        }
    }

    fn specialize_mir(&mut self, mir: &mut Mir) {
        let start = Instant::now();

        Specialize::run_all(&mut self.state, mir);
        self.timings.specialize_mir = start.elapsed();
    }

    fn optimise_mir(&mut self, mir: &mut Mir) {
        let start = Instant::now();

        measure(&mut self.timings.optimize.prepare, || {
            // Lowering from HIR to MIR may produce empty blocks, which we don't
            // want for future passes. This pass also automatically removes
            // unreachable blocks.
            mir.remove_empty_blocks();

            // Other passes depend on basic blocks ending with a terminator
            // instruction, so let's make sure this is actually the case.
            mir.terminate_basic_blocks();
        });

        // These passes are optional and thus only enabled if optimizations are
        // enabled.
        if !matches!(self.state.config.opt, Opt::Debug) {
            measure(&mut self.timings.optimize.inline, || {
                InlineMethod::run_all(&mut self.state, mir);
            });

            // After inlining it's possible certain methods that can't be called
            // through dynamic dispatch are all inlined, in which case there's
            // no point in keeping them around.
            measure(&mut self.timings.optimize.remove_methods, || {
                mir.remove_unused_methods(&self.state.db);
            });

            measure(&mut self.timings.optimize.method_local, || {
                mir.apply_method_local_optimizations(self.state.config.threads);
            });

            mir.remove_unused_constants(&self.state.db);
        }

        self.timings.optimize.total = start.elapsed();
    }

    fn write_dot(
        &self,
        directories: &BuildDirectories,
        mir: &Mir,
    ) -> Result<(), CompileError> {
        directories.create_dot().map_err(CompileError::Internal)?;

        for module in mir.modules.values() {
            if module.methods.is_empty() {
                continue;
            }

            let methods: Vec<_> = module
                .methods
                .iter()
                .map(|m| mir.methods.get(m).unwrap())
                .collect();

            let dot = to_dot(&self.state.db, &methods);
            let name = module.id.name(&self.state.db);
            let mut path = directories.dot.join(module_debug_path(name));

            path.set_extension("dot");

            let res = if let Some(dir) = path.parent() {
                create_dir_all(dir)
            } else {
                Ok(())
            };

            res.and_then(|_| write(&path, dot)).map_err(|err| {
                CompileError::Internal(format!(
                    "failed to write to {}: {}",
                    path.display(),
                    err
                ))
            })?;
        }

        Ok(())
    }

    fn compile_machine_code(
        &mut self,
        directories: &BuildDirectories,
        mir: Mir,
        symbols: &SymbolNames,
        main_file: PathBuf,
    ) -> Result<PathBuf, CompileError> {
        let start = Instant::now();
        let name = if let Some(v) =
            main_file.file_stem().map(|s| s.to_string_lossy().into_owned())
        {
            v
        } else {
            return Err(CompileError::Internal(format!(
                "failed to derive the executable name from {}",
                main_file.display()
            )));
        };

        let exe = directories.bin.join(name);
        let mut res =
            llvm::passes::lower_all(&mut self.state, directories, mir, symbols)
                .map_err(CompileError::Internal)?;

        self.timings.llvm = start.elapsed();
        self.timings.llvm_modules.append(&mut res.timings);
        self.timings.llvm_modules.sort_by(|a, b| a.0.cmp(&b.0));

        let start = Instant::now();

        link(&self.state, &exe, &res.objects, directories)
            .map_err(CompileError::Internal)?;
        self.timings.link = start.elapsed();

        Ok(exe)
    }

    fn prepare(&mut self) -> Result<(), CompileError> {
        let cur_ver = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let iter = Manifest::all(&self.state.config)
            .map_err(CompileError::Internal)?;

        for manifest in iter {
            let Some(ver) = manifest.minimum_inko_version() else { continue };

            if ver > cur_ver {
                return Err(CompileError::Internal(format!(
                    "this project requires Inko {} or newer, \
                    but the current version is {}",
                    ver, cur_ver
                )));
            }
        }

        sync_if_needed(&self.state.config.dependencies)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_debug_path() {
        assert_eq!(module_debug_path(&ModuleName::new("")), PathBuf::new());
        assert_eq!(
            module_debug_path(&ModuleName::new("a")),
            PathBuf::from("a")
        );
        assert_eq!(
            module_debug_path(&ModuleName::new("a.b")),
            PathBuf::from("a/b")
        );
        assert_eq!(
            module_debug_path(&ModuleName::new("a.b.c")),
            PathBuf::from("a/b/c")
        );
        assert_eq!(
            module_debug_path(&ModuleName::new("a.b.c#foo.bar")),
            PathBuf::from("a/b/c#029fa720ef93772028396d0b41779e1ca4ef3ae659914822d16ff85574b7cf2b")
        );
        assert_eq!(
            module_debug_path(&ModuleName::new("c#foo.bar")),
            PathBuf::from("c#029fa720ef93772028396d0b41779e1ca4ef3ae659914822d16ff85574b7cf2b")
        );
        assert_eq!(
            module_debug_path(&ModuleName::new("a.b.c<closure>(a,1,2)")),
            PathBuf::from("a/b/c<closure>bd2fd1bac3b67b10585aa5ae8a79f359ec15d969c2962765799cf957ff40f468")
        );
    }
}
