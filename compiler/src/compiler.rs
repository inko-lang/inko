use crate::config::{BuildDirectories, Output};
use crate::config::{Config, SOURCE, SOURCE_EXT, TESTS};
use crate::hir;
use crate::linker::link;
use crate::llvm;
use crate::mir::passes as mir;
use crate::mir::printer::to_dot;
use crate::mir::specialize::Specialize;
use crate::mir::Mir;
use crate::modules_parser::{ModulesParser, ParsedModule};
use crate::state::State;
use crate::type_check::define_types::{
    CheckTraitImplementations, CheckTraitRequirements, CheckTypeParameters,
    DefineFields, DefineTraitRequirements, DefineTypeParameterRequirements,
    DefineTypeParameters, DefineTypes, DefineVariants, ImplementTraits,
    InsertPrelude,
};
use crate::type_check::expressions::{DefineConstants, Expressions};
use crate::type_check::imports::{CollectExternImports, DefineImportedTypes};
use crate::type_check::methods::{
    CheckMainMethod, DefineMethods, DefineModuleMethodNames,
    ImplementTraitMethods,
};
use std::env::current_dir;
use std::ffi::OsStr;
use std::fs::write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use types::module_name::ModuleName;

fn format_timing(duration: Duration, total: Option<Duration>) -> String {
    let base = if duration.as_secs() >= 1 {
        format!("{:.2} sec ", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{} msec", duration.as_millis())
    } else if duration.as_micros() >= 1 {
        format!("{} µsec", duration.as_micros())
    } else {
        format!("{} nsec", duration.as_nanos())
    };

    if let Some(total) = total {
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

struct Timings {
    ast: Duration,
    hir: Duration,
    type_check: Duration,
    mir: Duration,
    optimize_mir: Duration,
    llvm: Duration,
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
            optimize_mir: Duration::from_secs(0),
            llvm: Duration::from_secs(0),
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

pub struct Compiler {
    state: State,
    timings: Timings,
}

impl Compiler {
    pub fn new(config: Config) -> Self {
        Self { state: State::new(config), timings: Timings::new() }
    }

    pub fn check(&mut self, file: Option<PathBuf>) -> Result<(), CompileError> {
        let start = Instant::now();

        // When checking a project we want to fall back to checking _all_ files
        // including tests, not just the main module.
        //
        // We don't define the main module, as this allows for type-checking
        // libraries, which won't provide such a module.
        let input = if let Some(file) = file {
            let file = file.canonicalize().unwrap_or(file);

            vec![(self.module_name_from_path(&file), file)]
        } else {
            self.all_source_modules()?
        };

        let ast = self.parse(input);
        let hir = self.compile_hir(ast)?;
        let res = self.compile_mir(hir).map(|_| ());

        self.timings.total = start.elapsed();
        res
    }

    pub fn build(
        &mut self,
        file: Option<PathBuf>,
    ) -> Result<PathBuf, CompileError> {
        let start = Instant::now();
        let file = self.main_module_path(file)?;
        let main_mod = self.state.db.main_module().unwrap().clone();
        let ast = self.parse(vec![(main_mod, file.clone())]);
        let hir = self.compile_hir(ast)?;
        let mut mir = self.compile_mir(hir)?;

        self.optimise_mir(&mut mir);

        let dirs = BuildDirectories::new(&self.state.config);

        dirs.create().map_err(CompileError::Internal)?;

        if self.state.config.dot {
            self.write_dot(&dirs, &mir)?;
        }

        let res = self.compile_machine_code(&dirs, &mir, file);

        self.timings.total = start.elapsed();
        res
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
\x1b[1mStage\x1b[0m            \x1b[1mTime\x1b[0m
Source to AST    {ast}
AST to HIR       {hir}
Type check       {type_check}
HIR to MIR       {mir}
Optimize MIR     {optimize}
Generate LLVM    {llvm}
Link             {link}
Total            {total}\
            ",
            ast = format_timing(self.timings.ast, Some(total)),
            hir = format_timing(self.timings.hir, Some(total)),
            type_check = format_timing(self.timings.type_check, Some(total)),
            mir = format_timing(self.timings.mir, Some(total)),
            optimize = format_timing(self.timings.optimize_mir, Some(total)),
            llvm = format_timing(self.timings.llvm, Some(total)),
            link = format_timing(self.timings.link, Some(total)),
            total = format_timing(self.timings.total, None),
        );
    }

    pub fn create_build_directory(&self) -> Result<(), String> {
        BuildDirectories::new(&self.state.config).create_build()
    }

    fn main_module_path(
        &mut self,
        file: Option<PathBuf>,
    ) -> Result<PathBuf, CompileError> {
        let path = if let Some(file) = file {
            file.canonicalize().unwrap_or(file)
        } else {
            let main = self.state.config.main_source_module();

            if main.is_file() {
                main
            } else {
                let cwd = current_dir().unwrap_or_else(|_| PathBuf::new());
                let main_relative = main
                    .strip_prefix(cwd)
                    .unwrap_or(main.as_path())
                    .to_string_lossy()
                    .into_owned();

                return Err(CompileError::Internal(format!(
                    "You didn't specify a file to compile, nor can we fall \
                    back to '{}' as it doesn't exist",
                    main_relative,
                )));
            }
        };

        self.state.db.set_main_module(self.module_name_from_path(&path));
        Ok(path)
    }

    fn compile_mir(
        &mut self,
        mut modules: Vec<hir::Module>,
    ) -> Result<Mir, CompileError> {
        if !self.check_types(&mut modules) {
            return Err(CompileError::Invalid);
        }

        let start = Instant::now();
        let mut mir = Mir::new();
        let state = &mut self.state;

        mir::check_global_limits(state).map_err(CompileError::Internal)?;

        let ok = mir::DefineConstants::run_all(state, &mut mir, &modules)
            && mir::LowerToMir::run_all(state, &mut mir, modules);

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

    fn check_types(&mut self, modules: &mut Vec<hir::Module>) -> bool {
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
            && DefineVariants::run_all(state, modules)
            && DefineFields::run_all(state, modules)
            && DefineMethods::run_all(state, modules)
            && CheckMainMethod::run(state)
            && ImplementTraitMethods::run_all(state, modules)
            && DefineConstants::run_all(state, modules)
            && Expressions::run_all(state, modules);

        self.timings.type_check = start.elapsed();
        res
    }

    fn optimise_mir(&mut self, mir: &mut Mir) {
        let start = Instant::now();

        Specialize::run_all(&mut self.state, mir);
        mir::clean_up_basic_blocks(mir);
        self.timings.optimize_mir = start.elapsed();
    }

    fn write_dot(
        &self,
        directories: &BuildDirectories,
        mir: &Mir,
    ) -> Result<(), CompileError> {
        directories.create_dot().map_err(CompileError::Internal)?;

        for module in mir.modules.values() {
            let methods: Vec<_> =
                module.methods.iter().map(|m| &mir.methods[m]).collect();

            let output = to_dot(&self.state.db, mir, &methods);
            let name = module.id.name(&self.state.db).normalized_name();
            let path = directories.dot.join(format!("{}.dot", name));

            write(&path, output).map_err(|err| {
                CompileError::Internal(format!(
                    "Failed to write {}: {}",
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
        mir: &Mir,
        main_file: PathBuf,
    ) -> Result<PathBuf, CompileError> {
        let start = Instant::now();
        let exe = match &self.state.config.output {
            Output::Derive => {
                let name = main_file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "main".to_string());

                directories.bin.join(name)
            }
            Output::File(name) => directories.bin.join(name),
            Output::Path(path) => path.clone(),
        };

        let objects =
            llvm::passes::Compile::run_all(&self.state, directories, mir)
                .map_err(CompileError::Internal)?;

        self.timings.llvm = start.elapsed();

        let start = Instant::now();

        link(&self.state, &exe, &objects).map_err(CompileError::Internal)?;
        self.timings.link = start.elapsed();

        Ok(exe)
    }

    fn module_name_from_path(&self, file: &Path) -> ModuleName {
        file.strip_prefix(&self.state.config.source)
            .ok()
            .or_else(|| file.strip_prefix(&self.state.config.tests).ok())
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

    fn all_source_modules(
        &self,
    ) -> Result<Vec<(ModuleName, PathBuf)>, CompileError> {
        let mut modules = Vec::new();
        let mut paths = Vec::new();
        let src_ext = OsStr::new(SOURCE_EXT);
        let source = &self.state.config.source;
        let tests = &self.state.config.tests;

        if source.is_dir() {
            paths.push(source.clone());
        }

        if tests.is_dir() {
            paths.push(tests.clone());
        }

        while let Some(path) = paths.pop() {
            let iter = path.read_dir().map_err(|err| {
                CompileError::Internal(format!(
                    "Failed to read directory {:?}: {}",
                    path, err
                ))
            })?;

            for entry in iter {
                let path = entry
                    .map_err(|err| {
                        CompileError::Internal(format!(
                            "Failed to read the contents of {:?}: {}",
                            path, err
                        ))
                    })?
                    .path();

                if path.is_dir() {
                    paths.push(path);
                } else if path.is_file() && path.extension() == Some(src_ext) {
                    modules.push((self.module_name_from_path(&path), path));
                }
            }
        }

        Ok(modules)
    }
}
