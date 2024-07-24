use crate::config::{BuildDirectories, Opt};
use crate::llvm::builder::Builder;
use crate::llvm::constants::{
    ARRAY_BUF_INDEX, ARRAY_CAPA_INDEX, ARRAY_LENGTH_INDEX,
    CLASS_METHODS_COUNT_INDEX, CLASS_METHODS_INDEX, CLOSURE_CALL_INDEX,
    CONTEXT_ARGS_INDEX, DROPPER_INDEX, FIELD_OFFSET, HEADER_CLASS_INDEX,
    HEADER_REFS_INDEX, MESSAGE_ARGUMENTS_INDEX, METHOD_FUNCTION_INDEX,
    METHOD_HASH_INDEX, PROCESS_FIELD_OFFSET, STACK_DATA_EPOCH_INDEX,
    STACK_DATA_PROCESS_INDEX, STATE_EPOCH_INDEX,
};
use crate::llvm::context::Context;
use crate::llvm::layouts::Layouts;
use crate::llvm::methods::Methods;
use crate::llvm::module::Module;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::mir::{
    CastType, Constant, Instruction, LocationId, Method, Mir,
    Module as MirModule, RegisterId,
};
use crate::state::State;
use crate::symbol_names::{
    shapes, SymbolNames, STACK_MASK_GLOBAL, STATE_GLOBAL,
};
use crate::target::Architecture;
use blake3::{hash, Hasher};
use inkwell::basic_block::BasicBlock;
use inkwell::module::Linkage;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target,
    TargetMachine, TargetTriple,
};
use inkwell::types::{
    BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType,
};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FloatValue,
    FunctionValue, GlobalValue, IntValue, PointerValue,
};
use inkwell::AddressSpace;
use inkwell::OptimizationLevel;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{read, write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::scope;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use types::module_name::ModuleName;
use types::{
    BuiltinFunction, ClassId, Database, Module as ModuleType, Shape, TypeRef,
    BYTE_ARRAY_ID, STRING_ID,
};

fn object_path(directories: &BuildDirectories, name: &ModuleName) -> PathBuf {
    let hash = hash(name.as_str().as_bytes()).to_string();

    directories.objects.join(format!("{}.o", hash))
}

fn hash_compile_time_variables(state: &State) -> String {
    let mut hasher = Hasher::new();
    let mut pairs: Vec<_> =
        state.config.compile_time_variables.iter().collect();

    pairs.sort_by_key(|p| p.0);

    for ((mod_name, const_name), val) in pairs {
        hasher.update(mod_name.as_str().as_bytes());
        hasher.update(const_name.as_bytes());
        hasher.update(val.as_bytes());
    }

    hasher.finalize().to_string()
}

fn check_object_cache(
    state: &mut State,
    symbol_names: &SymbolNames,
    methods: &Methods,
    directories: &BuildDirectories,
    object_paths: &[PathBuf],
    mir: &Mir,
) -> Result<(), String> {
    let now = SystemTime::now();
    let mut force = !state.config.incremental
        || state.config.write_llvm
        || state.config.verify_llvm;

    // We don't have a stable ABI of any sort, so we force a flush every time
    // the compiler's executable is compiled again. This may be overly
    // conservative, but it ensures we don't end up using existing object files
    // that are in a state the current compiler version doesn't expect.
    //
    // We also take into account the version number, in case somebody tries to
    // compile a previously compiled project using an older version of the
    // compiler.
    let time = state
        .config
        .compiled_at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let vars_hash = hash_compile_time_variables(state);
    let new_ver =
        format!("{}-{}-{}", env!("CARGO_PKG_VERSION"), time, vars_hash);
    let ver_path = directories.objects.join("version");
    let ver_changed = if ver_path.is_file() {
        read(&ver_path)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map_or(true, |old_ver| old_ver != new_ver)
    } else {
        true
    };

    if ver_changed {
        force = true;
        write(&ver_path, new_ver).map_err(|e| {
            format!(
                "failed to write the compiler version to {}: {}",
                ver_path.display(),
                e
            )
        })?;
    }

    for (module, obj_path) in mir.modules.values().iter().zip(object_paths) {
        let name = module.id.name(&state.db);
        let src_path = module.id.file(&state.db);
        let mut changed = force;

        // We only check the timestamp if we aren't forced to flush the cache
        // already.
        if !changed {
            let src_time =
                src_path.metadata().and_then(|m| m.modified()).unwrap_or(now);

            // We default to the Unix epoch such that a missing object file is
            // treated as one created in 1970. This way we don't need to wrap
            // things in an extra Option or a similar type.
            let obj_time = obj_path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);

            changed = src_time > obj_time;
        }

        // It's possible the source file remains unchanged relative to the
        // object file, but the list of symbols we wish to write to the object
        // file has changed. This can happen if module A imports module B, B
        // introduces some generic type used in A, and A introduces a new
        // specialization of that type.
        //
        // We solve this by maintaining hashes of the sorted list of symbol
        // names, and compare those between compilations to see if the list has
        // changed. If so, we flush the cache.
        //
        // An alternative approach involves reading the symbol names from
        // existing object files, sorting those and comparing them to the list
        // we build here. This likely would be much slower though, as object
        // files can get pretty large and modules can easily have hundreds of
        // symbol names. Blake2 on the other hand is fast enough, and we only
        // need to perform one final comparison instead of potentially many
        // comparisons per module.
        let mut names = Vec::new();

        for id in &module.constants {
            names.push(&symbol_names.constants[id]);
        }

        for id in &module.classes {
            names.push(&symbol_names.classes[id]);
        }

        for id in &module.methods {
            names.push(&symbol_names.methods[id]);
        }

        names.sort();

        let mut hasher = Hasher::new();

        for name in names {
            hasher.update(name.as_bytes());
        }

        // The module may contain dynamic dispatch call sites. If the need for
        // probing changes, we need to update the module's code accordingly. We
        // do this by hashing the collision states of all dynamic calls in the
        // current module, such that if any of them change, so does the hash.
        for &mid in &module.methods {
            hasher.update(&methods.info[mid.0 as usize].hash.to_le_bytes());

            for block in &mir.methods[&mid].body.blocks {
                for ins in &block.instructions {
                    if let Instruction::CallDynamic(op) = ins {
                        let val = methods.info[op.method.0 as usize].collision;

                        hasher.update(&[val as u8]);
                    }
                }
            }
        }

        let new_hash = format!("{}", hasher.finalize());
        let hash_path = obj_path.with_extension("o.blake3");

        // We don't need to perform this check if another check already
        // determined the object file needs to be refreshed.
        //
        // If we can't read the file for some reason or its contents are
        // invalid, the only safe assumption we can make is that the object file
        // has changed.
        if !changed {
            changed = if hash_path.is_file() {
                read(&hash_path)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok())
                    .map_or(true, |old_hash| old_hash != new_hash)
            } else {
                true
            };
        }

        if !changed {
            continue;
        }

        // We only need to write to the hash file if there are any changes.
        write(&hash_path, new_hash).map_err(|err| {
            format!(
                "failed to write the object file hash to {}: {}",
                hash_path.display(),
                err,
            )
        })?;

        let mut work = vec![state.dependency_graph.module_id(name).unwrap()];

        while let Some(id) = work.pop() {
            if state.dependency_graph.mark_as_changed(id) {
                work.append(&mut state.dependency_graph.depending(id));
            }
        }
    }

    Ok(())
}

fn sort_mir(db: &Database, mir: &mut Mir, names: &SymbolNames) {
    // We sort the data by their generated symbol names, as these are already
    // unique for each ID and take into account data such as the shapes. If we
    // sorted just by IDs we'd get an inconsistent order between compilations,
    // and if we just sorted by names we may get an inconsistent order when many
    // values share the same name.
    for module in mir.modules.values_mut() {
        module.constants.sort_by_key(|i| &names.constants[i]);
        module.classes.sort_by_key(|i| &names.classes[i]);
        module.methods.sort_by_key(|i| &names.methods[i]);
    }

    for class in mir.classes.values_mut() {
        class.methods.sort_by_key(|i| &names.methods[i]);
    }

    // When populating object caches we need to be able to iterate over the MIR
    // modules in a stable order. We do this here (and once) such that from this
    // point forward, we can rely on a stable order, as it's too easy to forget
    // to first sort this list every time we want to iterate over it.
    //
    // Because `mir.modules` is an IndexMap, sorting it is a bit more involved
    // compared to just sorting a `Vec`.
    let mut values = mir.modules.take_values();

    values.sort_by_key(|m| m.id.name(db));

    for module in values {
        mir.modules.insert(module.id, module);
    }
}

/// A pass that splits modules into smaller ones, such that each specialized
/// type has its own module.
///
/// This pass is used to make caching and parallel compilation more effective,
/// such that adding a newly specialized type won't flush many caches
/// unnecessarily.
pub(crate) fn split_modules(
    state: &mut State,
    mir: &mut Mir,
) -> Result<(), String> {
    let mut new_modules = Vec::new();

    for old_module in mir.modules.values_mut() {
        let mut moved_classes = HashSet::new();
        let mut moved_methods = HashSet::new();

        for &class_id in &old_module.classes {
            if class_id.specialization_source(&state.db).unwrap_or(class_id)
                == class_id
                || class_id.kind(&state.db).is_closure()
            {
                // Non-generic and closure classes always originate from the
                // source modules, so there's no need to move them elsewhere.
                continue;
            }

            let file = old_module.id.file(&state.db);
            let orig_name = old_module.id.name(&state.db).clone();
            let name = ModuleName::new(format!(
                "{}({}#{})",
                orig_name,
                class_id.name(&state.db),
                shapes(class_id.shapes(&state.db))
            ));

            let new_mod_id =
                ModuleType::alloc(&mut state.db, name.clone(), file);

            // For symbols/stack traces we want to use the original name, not
            // the generated one.
            new_mod_id.set_method_symbol_name(&mut state.db, orig_name.clone());

            // We have to record the new module in the dependency graph, that
            // way a change to the original module also affects these generated
            // modules.
            let new_node_id = state.dependency_graph.add_module(name);
            let old_node_id =
                state.dependency_graph.module_id(&orig_name).unwrap();

            state.dependency_graph.add_depending(old_node_id, new_node_id);

            let mut new_module = MirModule::new(new_mod_id);

            // We don't deal with static methods as those have their receiver
            // typed as the original class ID, because they don't really belong
            // to a class (i.e. they're basically scoped module methods).
            new_module.methods = mir.classes[&class_id].methods.clone();
            new_module.classes.push(class_id);
            moved_classes.insert(class_id);

            // When generating symbol names we use the module as stored in the
            // method, so we need to make sure that's set to our newly generated
            // module.
            for &id in &new_module.methods {
                id.set_module(&mut state.db, new_mod_id);
                moved_methods.insert(id);
            }

            class_id.set_module(&mut state.db, new_mod_id);
            new_modules.push(new_module);
        }

        old_module.methods.retain(|id| !moved_methods.contains(id));
        old_module.classes.retain(|i| !moved_classes.contains(i));
    }

    for module in new_modules {
        mir.modules.insert(module.id, module);
    }

    Ok(())
}

/// Compiles all the modules into object files.
///
/// The return value is a list of file paths to the generated object files.
pub(crate) fn lower_all(
    state: &mut State,
    directories: &BuildDirectories,
    mut mir: Mir,
) -> Result<CompileResult, String> {
    let names = SymbolNames::new(&state.db, &mir);

    sort_mir(&state.db, &mut mir, &names);

    let methods = Methods::new(&state.db, &mir);

    // The object paths are generated using Blake2, and are needed in several
    // places. We generate them once here, then reuse the data by indexing this
    // Vec based on the index of the MIR module being processed.
    let obj_paths: Vec<PathBuf> = mir
        .modules
        .values()
        .iter()
        .map(|m| object_path(directories, m.id.name(&state.db)))
        .collect();

    check_object_cache(state, &names, &methods, directories, &obj_paths, &mir)?;

    if state.config.write_llvm {
        directories.create_llvm()?;
    }

    match state.config.target.arch {
        Architecture::Amd64 => {
            Target::initialize_x86(&InitializationConfig::default());
        }
        Architecture::Arm64 => {
            Target::initialize_aarch64(&InitializationConfig::default());
        }
    }

    // LLVM's optimisation level controls which passes to run, but some/many of
    // those may not be relevant to Inko, while slowing down compile times. Thus
    // instead of using this knob, we provide our own list of passes. Swift and
    // Rust (and possibly others) take a similar approach.
    //
    // For the aggressive mode we simply enable the full suite of LLVM
    // optimizations, likely greatly increasing the compilation times.
    let level = match state.config.opt {
        Opt::None => OptimizationLevel::None,

        // We have yet to figure out what optimizations we want to enable
        // here, hence we don't apply any at all.
        Opt::Balanced => OptimizationLevel::None,

        // This is the equivalent of -O3 for clang.
        Opt::Aggressive => OptimizationLevel::Aggressive,
    };

    // Our "queue" is just an atomic integer in the range 0..N where N is the
    // number of MIR modules. These integers are then used to index the list of
    // MIR modules, removing the need for some sort of synchronized queue.
    let queue = AtomicUsize::new(0);
    let shared = SharedState {
        state,
        mir: &mir,
        methods: &methods,
        names: &names,
        queue: &queue,
        directories,
        object_paths: &obj_paths,
        level,
    };

    let mut paths = Vec::with_capacity(mir.modules.len());
    let mut timings = Vec::with_capacity(mir.modules.len());

    scope(|s| -> Result<(), String> {
        let handles: Vec<_> = (0..shared.state.config.threads)
            .map(|i| {
                let shared = &shared;

                s.spawn(move || Worker::new(shared, i == 0).run())
            })
            .collect();

        for handle in handles {
            // If the thread panics we don't need to panic here _again_, as that
            // just clutters the output.
            if let Ok(res) = handle.join() {
                let mut res = res?;

                paths.append(&mut res.paths);
                timings.extend(res.timings.into_iter());
            }
        }

        Ok(())
    })?;

    // To ensure the resulting executable is the same between different
    // compilations (assuming no code changes), we sort the paths so they're
    // provided to the linker in a consistent order. This makes inspecting the
    // resulting executable easier, as the code locations don't randomly change
    // based on what order object files are linked in.
    paths.sort();

    Ok(CompileResult { objects: paths, timings })
}

pub(crate) struct CompileResult {
    /// The file paths to the generated object files to link together.
    pub(crate) objects: Vec<PathBuf>,

    /// The timings of each module that is compiled.
    pub(crate) timings: Vec<(ModuleName, Duration)>,
}

/// The state shared between worker threads.
struct SharedState<'a> {
    state: &'a State,
    mir: &'a Mir,
    methods: &'a Methods,
    names: &'a SymbolNames,
    queue: &'a AtomicUsize,
    directories: &'a BuildDirectories,
    object_paths: &'a Vec<PathBuf>,
    level: OptimizationLevel,
}

struct WorkerResult {
    timings: HashMap<ModuleName, Duration>,
    paths: Vec<PathBuf>,
}

/// A worker thread for turning MIR modules into object files.
struct Worker<'a> {
    shared: &'a SharedState<'a>,
    machine: TargetMachine,
    timings: HashMap<ModuleName, Duration>,

    /// If this worker should also generate the main module that sets everything
    /// up.
    main: bool,
}

impl<'a> Worker<'a> {
    fn new(shared: &'a SharedState<'a>, main: bool) -> Worker<'a> {
        let reloc = RelocMode::PIC;
        let model = CodeModel::Default;
        let triple_name = shared.state.config.target.llvm_triple();
        let triple = TargetTriple::create(&triple_name);
        let machine = Target::from_triple(&triple)
            .unwrap()
            .create_target_machine(&triple, "", "", shared.level, reloc, model)
            .unwrap();

        Worker { shared, main, machine, timings: HashMap::new() }
    }

    fn run(mut self) -> Result<WorkerResult, String> {
        let mut paths = Vec::new();
        let max = self.shared.mir.modules.len();

        // This data can't be stored in `self` as `Layouts` retains references
        // to `Context`, preventing it from being moved into `self`. Thus, we
        // create this data here and pass it as arguments.
        let context = Context::new();
        let target_data = self.machine.get_target_data();
        let layouts = Layouts::new(
            self.shared.state,
            self.shared.mir,
            &context,
            &target_data,
        );

        loop {
            let index = self.shared.queue.fetch_add(1, Ordering::AcqRel);

            if index >= max {
                break;
            }

            paths.push(self.lower(index, &context, &layouts)?);
        }

        if self.main {
            paths.push(self.generate_main(&context, &layouts)?);
        }

        Ok(WorkerResult { paths, timings: self.timings })
    }

    fn lower(
        &mut self,
        index: usize,
        context: &Context,
        layouts: &Layouts,
    ) -> Result<PathBuf, String> {
        let start = Instant::now();
        let mod_id = self.shared.mir.modules[index].id;
        let name = mod_id.name(&self.shared.state.db);
        let obj_path = self.shared.object_paths[index].clone();

        if !self.shared.state.dependency_graph.module_changed(name) {
            self.timings.insert(name.clone(), start.elapsed());
            return Ok(obj_path);
        }

        let path = mod_id.file(&self.shared.state.db);
        let mut module = Module::new(context, layouts, name.clone(), &path);

        LowerModule {
            shared: self.shared,
            index,
            module: &mut module,
            layouts,
        }
        .run();

        let res = self.process_module(&module, layouts, obj_path);

        self.timings.insert(name.clone(), start.elapsed());
        res
    }

    fn generate_main(
        &mut self,
        context: &Context,
        layouts: &Layouts,
    ) -> Result<PathBuf, String> {
        let start = Instant::now();
        let name = ModuleName::new("$main");
        let path = Path::new("$main.inko");
        let main = Module::new(context, layouts, name.clone(), path);

        GenerateMain::new(
            &self.shared.state.db,
            self.shared.mir,
            layouts,
            self.shared.methods,
            self.shared.names,
            &main,
        )
        .run();

        let path = object_path(self.shared.directories, &name);
        let res = self.process_module(&main, layouts, path);

        self.timings.insert(name, start.elapsed());
        res
    }

    fn process_module(
        &self,
        module: &Module,
        layouts: &Layouts,
        path: PathBuf,
    ) -> Result<PathBuf, String> {
        self.run_passes(module, layouts);
        self.write_object_file(module, path)
    }

    fn run_passes(&self, module: &Module, layouts: &Layouts) {
        let layout = layouts.target_data.get_data_layout();
        let opts = PassBuilderOptions::create();
        let passes = if let Opt::Aggressive = self.shared.state.config.opt {
            &["default<O3>"]
        } else {
            &["mem2reg"]
        };

        module.set_data_layout(&layout);
        module.set_triple(&self.machine.get_triple());
        module
            .run_passes(passes.join(",").as_str(), &self.machine, opts)
            .unwrap();
    }

    fn write_object_file(
        &self,
        module: &Module,
        path: PathBuf,
    ) -> Result<PathBuf, String> {
        if self.shared.state.config.write_llvm {
            let name = module.name.normalized_name();
            let path =
                self.shared.directories.llvm_ir.join(format!("{}.ll", name));

            module.print_to_file(&path).map_err(|e| {
                format!("failed to write LLVM IR to {}: {}", path.display(), e)
            })?;
        }

        // We verify _after_ writing the IR such that one can inspect the IR in
        // the event it's invalid.
        if self.shared.state.config.verify_llvm {
            module.verify().map_err(|e| {
                format!(
                    "the LLVM module '{}' is invalid:\n\n{}\n",
                    module.name,
                    e.to_string()
                )
            })?;
        }

        self.machine
            .write_to_file(&module.inner, FileType::Object, &path)
            .map_err(|e| {
                format!("failed to write object file {}: {}", path.display(), e)
            })
            .map(|_| path)
    }
}

/// A pass that lowers a single Inko module into an LLVM module.
pub(crate) struct LowerModule<'shared, 'module, 'ctx> {
    shared: &'shared SharedState<'shared>,
    index: usize,
    layouts: &'ctx Layouts<'ctx>,
    module: &'module mut Module<'shared, 'ctx>,
}

impl<'shared, 'module, 'ctx> LowerModule<'shared, 'module, 'ctx> {
    pub(crate) fn run(mut self) {
        for method in &self.shared.mir.modules[self.index].methods {
            LowerMethod::new(
                self.shared,
                self.layouts,
                self.module,
                &self.shared.mir.methods[method],
            )
            .run();
        }

        self.setup_classes();
        self.setup_constants();
        self.module.debug_builder.finalize();
    }

    fn setup_classes(&mut self) {
        let mod_id = self.shared.mir.modules[self.index].id;
        let space = AddressSpace::default();
        let fn_name = &self.shared.names.setup_classes[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.module.context, fn_val);
        let entry_block = self.module.context.append_basic_block(fn_val);

        builder.switch_to_block(entry_block);

        let state = self.load_state(&builder);
        let body = self.module.context.append_basic_block(fn_val);

        builder.jump(body);
        builder.switch_to_block(body);

        // Allocate all classes defined in this module, and store them in their
        // corresponding globals.
        for &class_id in &self.shared.mir.modules[self.index].classes {
            let raw_name = class_id.name(&self.shared.state.db);
            let name_ptr = builder.string_literal(raw_name).0.into();
            let methods_len = self
                .module
                .context
                .i16_type()
                .const_int(
                    self.shared.methods.counts[class_id.0 as usize] as _,
                    false,
                )
                .into();

            let class_new = if class_id.kind(&self.shared.state.db).is_async() {
                self.module.runtime_function(RuntimeFunction::ClassProcess)
            } else {
                self.module.runtime_function(RuntimeFunction::ClassObject)
            };

            let layout = self.layouts.classes[class_id.0 as usize];
            let global_name = &self.shared.names.classes[&class_id];
            let global = self.module.add_class(class_id, global_name);

            // The class globals must have an initializer, otherwise LLVM treats
            // them as external globals.
            global.set_initializer(
                &layout.ptr_type(space).const_null().as_basic_value_enum(),
            );

            // Built-in classes are defined in the runtime library, so we should
            // look them up instead of creating a new one.
            let class_ptr = match class_id.0 {
                STRING_ID => builder
                    .load_field(self.layouts.state, state, 0)
                    .into_pointer_value(),
                BYTE_ARRAY_ID => builder
                    .load_field(self.layouts.state, state, 1)
                    .into_pointer_value(),
                _ => {
                    let size = builder.int_to_int(
                        self.layouts.instances[class_id.0 as usize]
                            .size_of()
                            .unwrap(),
                        32,
                        false,
                    );

                    builder
                        .call(class_new, &[name_ptr, size.into(), methods_len])
                        .into_pointer_value()
                }
            };

            for method in &self.shared.mir.classes[&class_id].methods {
                // Static methods aren't stored in classes, nor can we call them
                // through dynamic dispatch, so we can skip the rest.
                if method.is_static(&self.shared.state.db) {
                    continue;
                }

                let info = &self.shared.methods.info[method.0 as usize];
                let name = &self.shared.names.methods[method];
                let func = self
                    .module
                    .get_function(name)
                    .unwrap()
                    .as_global_value()
                    .as_pointer_value();

                let slot = builder.u32_literal(info.index as u32);
                let method_addr = builder.array_field_index_address(
                    self.layouts.empty_class,
                    class_ptr,
                    CLASS_METHODS_INDEX,
                    slot,
                );

                let hash = builder.u64_literal(info.hash);
                let layout = self.layouts.method;
                let hash_idx = METHOD_HASH_INDEX;
                let func_idx = METHOD_FUNCTION_INDEX;
                let var = builder.new_temporary(self.layouts.method);

                builder.store_field(layout, var, hash_idx, hash);
                builder.store_field(layout, var, func_idx, func);

                let method = builder.load(layout, var);

                builder.store(method_addr, method);
            }

            builder.store(global.as_pointer_value(), class_ptr);
        }

        builder.return_value(None);
    }

    fn setup_constants(&mut self) {
        let mod_id = self.shared.mir.modules[self.index].id;
        let fn_name = &self.shared.names.setup_constants[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.module.context, fn_val);
        let entry_block = self.module.context.append_basic_block(fn_val);

        builder.switch_to_block(entry_block);

        let state = self.load_state(&builder);
        let body = self.module.context.append_basic_block(fn_val);

        builder.jump(body);
        builder.switch_to_block(body);

        for &cid in &self.shared.mir.modules[self.index].constants {
            let name = &self.shared.names.constants[&cid];
            let global = self.module.add_constant(name);
            let value = &self.shared.mir.constants[&cid];

            global.set_initializer(
                &self
                    .module
                    .context
                    .pointer_type()
                    .const_null()
                    .as_basic_value_enum(),
            );
            self.set_constant_global(&builder, state, value, global);
        }

        // We sort this list so different compilations always produce this list
        // in a consistent order, making it easier to compare the output of
        // incremental vs non-incremental builds.
        let mut strings: Vec<_> = self.module.strings.iter().collect();

        strings.sort_by_key(|p| p.0);

        for (value, global) in strings {
            let ptr = global.as_pointer_value();
            let val = self.new_string(&builder, state, value);

            builder.store(ptr, val);
        }

        builder.return_value(None);
    }

    fn set_constant_global(
        &mut self,
        builder: &Builder<'ctx>,
        state: PointerValue<'ctx>,
        constant: &Constant,
        global: GlobalValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let global = global.as_pointer_value();
        let value = self.permanent_value(builder, state, constant);

        builder.store(global, value);
        global
    }

    fn permanent_value(
        &mut self,
        builder: &Builder<'ctx>,
        state: PointerValue<'ctx>,
        constant: &Constant,
    ) -> BasicValueEnum<'ctx> {
        match constant {
            Constant::Int(val) => {
                builder.i64_literal(*val).as_basic_value_enum()
            }
            Constant::Float(val) => {
                builder.f64_literal(*val).as_basic_value_enum()
            }
            Constant::String(val) => self.new_string(builder, state, val),
            Constant::Bool(true) => {
                builder.i64_literal(1).as_basic_value_enum()
            }
            Constant::Bool(false) => {
                builder.i64_literal(0).as_basic_value_enum()
            }
            Constant::Array(values) => {
                let (shape, val_typ) = match values.first() {
                    Some(Constant::Int(_)) => (
                        Shape::Int,
                        builder.context.i64_type().as_basic_type_enum(),
                    ),
                    Some(Constant::Bool(_)) => (
                        Shape::Boolean,
                        builder.context.i64_type().as_basic_type_enum(),
                    ),
                    Some(Constant::Float(_)) => (
                        Shape::Float,
                        builder.context.f64_type().as_basic_type_enum(),
                    ),
                    Some(Constant::String(_)) => (
                        Shape::String,
                        builder.context.pointer_type().as_basic_type_enum(),
                    ),
                    Some(Constant::Array(_)) => (
                        Shape::Ref,
                        builder.context.pointer_type().as_basic_type_enum(),
                    ),
                    _ => (
                        Shape::Owned,
                        builder.context.pointer_type().as_basic_type_enum(),
                    ),
                };

                let class_id = ClassId::array()
                    .specializations(&self.shared.state.db)[&vec![shape]];
                let layout = self.layouts.instances[class_id.0 as usize];
                let array = builder.allocate(
                    self.module,
                    &self.shared.state.db,
                    self.shared.names,
                    class_id,
                );

                let buf_typ = val_typ.array_type(values.len() as _);

                // The memory of array constants is statically allocated, as we
                // never need to resize it. Using malloc() would also mean that
                // we'd need to handle it failing, which means triggering a
                // panic, which we can't do at this point as we don't have a
                // process set up yet.
                let buf_global = self.module.add_global(buf_typ, "");
                let buf_ptr = buf_global.as_pointer_value();

                // We use a private linkage so we don't need to generate a
                // globally unique symbol name for the buffer global.
                buf_global.set_linkage(Linkage::Private);
                buf_global.set_initializer(
                    &buf_typ.const_zero().as_basic_value_enum(),
                );

                for (index, arg) in values.iter().enumerate() {
                    let val = self.permanent_value(builder, state, arg);

                    builder
                        .store_array_field(buf_typ, buf_ptr, index as _, val);
                }

                let len = builder.i64_literal(values.len() as _);

                builder.store_field(layout, array, ARRAY_LENGTH_INDEX, len);
                builder.store_field(layout, array, ARRAY_CAPA_INDEX, len);
                builder.store_field(layout, array, ARRAY_BUF_INDEX, buf_ptr);
                array.as_basic_value_enum()
            }
        }
    }

    fn new_string(
        &self,
        builder: &Builder<'ctx>,
        state: PointerValue<'ctx>,
        value: &str,
    ) -> BasicValueEnum<'ctx> {
        let bytes_typ = builder.context.i8_type().array_type(value.len() as _);
        let bytes_var = builder.new_temporary(bytes_typ);
        let bytes = builder.string_bytes(value);

        builder.store(bytes_var, bytes);

        let len = builder.u64_literal(value.len() as u64).into();
        let func = self.module.runtime_function(RuntimeFunction::StringNew);

        builder.call(func, &[state.into(), bytes_var.into(), len])
    }

    fn load_state(&mut self, builder: &Builder<'ctx>) -> PointerValue<'ctx> {
        let state_global = self.module.add_constant(STATE_GLOBAL);

        builder
            .load_pointer(self.layouts.state, state_global.as_pointer_value())
    }
}

/// A pass that lowers a single Inko method into an LLVM method.
pub struct LowerMethod<'shared, 'module, 'ctx> {
    shared: &'shared SharedState<'shared>,
    layouts: &'ctx Layouts<'ctx>,

    /// The MIR method that we're lowering to LLVM.
    method: &'shared Method,

    /// The builder to use for generating instructions.
    builder: Builder<'ctx>,

    /// The LLVM module the generated code belongs to.
    module: &'module mut Module<'shared, 'ctx>,

    /// MIR registers and their corresponding LLVM stack variables.
    variables: HashMap<RegisterId, PointerValue<'ctx>>,

    /// The LLVM types for each MIR register.
    variable_types: HashMap<RegisterId, BasicTypeEnum<'ctx>>,
}

impl<'shared, 'module, 'ctx> LowerMethod<'shared, 'module, 'ctx> {
    fn new(
        shared: &'shared SharedState<'shared>,
        layouts: &'ctx Layouts<'ctx>,
        module: &'module mut Module<'shared, 'ctx>,
        method: &'shared Method,
    ) -> Self {
        let function =
            module.add_method(&shared.names.methods[&method.id], method.id);
        let builder = Builder::new(module.context, function);

        LowerMethod {
            shared,
            layouts,
            method,
            module,
            builder,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
        }
    }

    fn run(&mut self) {
        if self.method.id.is_async(&self.shared.state.db) {
            self.async_method();
        } else {
            self.regular_method();
        }
    }

    fn regular_method(&mut self) {
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);
        self.define_register_variables();

        for (arg, reg) in
            self.builder.arguments().zip(self.method.arguments.iter())
        {
            self.builder.store(self.variables[reg], arg);
        }

        let (line, _) =
            self.shared.mir.location(self.method.location).line_column();
        let debug_func = self.module.debug_builder.new_function(
            self.method.id.name(&self.shared.state.db),
            &self.shared.names.methods[&self.method.id],
            &self.method.id.source_file(&self.shared.state.db),
            line,
            self.method.id.is_private(&self.shared.state.db),
            false,
        );

        self.builder.set_debug_function(debug_func);
        self.method_body();
    }

    fn async_method(&mut self) {
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let space = AddressSpace::default();
        let num_args = self.method.arguments.len() as u32;
        let args_type =
            self.builder.context.pointer_type().array_type(num_args);
        let args_var = self.builder.new_stack_slot(args_type.ptr_type(space));
        let ctx_var =
            self.builder.new_stack_slot(self.layouts.context.ptr_type(space));

        self.define_register_variables();

        // Destructure the context into its components. This is necessary as the
        // context only lives until the first yield.
        self.builder.store(ctx_var, self.builder.argument(0));

        let ctx = self.builder.load_pointer(self.layouts.context, ctx_var);
        let args = self
            .builder
            .load_field(self.layouts.context, ctx, CONTEXT_ARGS_INDEX)
            .into_pointer_value();

        self.builder.store(args_var, args);

        // For async methods we don't include the receiver in the message, as
        // we can instead just read the process from the private stack data.
        let self_var = self.variables[&self.method.arguments[0]];
        let proc = self.load_process();

        self.builder.store(self_var, proc);

        // Populate the argument stack variables according to the values stored
        // in the context structure.
        for (index, reg) in self.method.arguments.iter().skip(1).enumerate() {
            let var = self.variables[reg];
            let args = self.builder.load_pointer(args_type, args_var);
            let val = self
                .builder
                .load_array_index(args_type, args, index)
                .into_pointer_value();

            self.builder.store(var, val);
        }

        let (line, _) =
            self.shared.mir.location(self.method.location).line_column();
        let debug_func = self.module.debug_builder.new_function(
            self.method.id.name(&self.shared.state.db),
            &self.shared.names.methods[&self.method.id],
            &self.method.id.source_file(&self.shared.state.db),
            line,
            self.method.id.is_private(&self.shared.state.db),
            false,
        );

        self.builder.set_debug_function(debug_func);
        self.method_body();
    }

    fn method_body(&mut self) {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut llvm_blocks = Vec::with_capacity(self.method.body.blocks.len());

        for _ in 0..self.method.body.blocks.len() {
            llvm_blocks.push(self.builder.add_block());
        }

        self.builder.jump(llvm_blocks[self.method.body.start_id.0]);

        queue.push_back(self.method.body.start_id);
        visited.insert(self.method.body.start_id);

        while let Some(block_id) = queue.pop_front() {
            let mir_block = &self.method.body.blocks[block_id.0];
            let llvm_block = llvm_blocks[block_id.0];

            self.builder.switch_to_block(llvm_block);

            for ins in &mir_block.instructions {
                self.instruction(&llvm_blocks, ins);
            }

            for &child in &mir_block.successors {
                if visited.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    fn instruction(&mut self, all_blocks: &[BasicBlock], ins: &Instruction) {
        match ins {
            Instruction::CallBuiltin(ins) => {
                self.set_debug_location(ins.location);

                match ins.name {
                    BuiltinFunction::IntDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_div(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRem => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_rem(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitAnd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_and(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitOr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_or(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitNot => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var);
                        let res = self.builder.bit_not(val);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitXor => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_xor(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let raw = self.builder.int_eq(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let raw = self.builder.int_gt(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let raw = self.builder.int_ge(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let raw = self.builder.int_le(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let raw = self.builder.int_lt(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_add(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_sub(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_div(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_mul(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatMod => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_rem(
                            self.builder.float_add(
                                self.builder.float_rem(lhs, rhs),
                                rhs,
                            ),
                            rhs,
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatCeil => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.ceil",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatFloor => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.floor",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let raw = self.builder.float_eq(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatToBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let res = self
                            .builder
                            .bitcast(val, self.builder.context.i64_type())
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatFromBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var);
                        let res = self
                            .builder
                            .bitcast(val, self.builder.context.f64_type())
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let raw = self.builder.float_gt(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let raw = self.builder.float_ge(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let raw = self.builder.float_lt(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let raw = self.builder.float_le(lhs, rhs);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatIsInf => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let fabs = self.module.intrinsic(
                            "llvm.fabs",
                            &[self.builder.context.f64_type().into()],
                        );

                        let pos_val = self
                            .builder
                            .call(fabs, &[val.into()])
                            .into_float_value();

                        let pos_inf = self.builder.f64_literal(f64::INFINITY);
                        let cond = self.builder.float_eq(pos_val, pos_inf);
                        let res = self.builder.bool_to_int(cond);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatIsNan => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let raw = self.builder.float_is_nan(val);
                        let res = self.builder.bool_to_int(raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatRound => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.round",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatPowi => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let raw_rhs = self.builder.load_int(rhs_var);
                        let rhs = self.builder.int_to_int(raw_rhs, 32, false);
                        let func = self.module.intrinsic(
                            "llvm.powi",
                            &[
                                self.builder.context.f64_type().into(),
                                self.builder.context.i32_type().into(),
                            ],
                        );

                        let res = self
                            .builder
                            .call(func, &[lhs.into(), rhs.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRotateLeft => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var).into();
                        let rhs = self.builder.load_int(rhs_var).into();
                        let func = self.module.intrinsic(
                            "llvm.fshl",
                            &[self.builder.context.i64_type().into()],
                        );
                        let res = self
                            .builder
                            .call(func, &[lhs, lhs, rhs])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRotateRight => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var).into();
                        let rhs = self.builder.load_int(rhs_var).into();
                        let func = self.module.intrinsic(
                            "llvm.fshr",
                            &[self.builder.context.i64_type().into()],
                        );
                        let res = self
                            .builder
                            .call(func, &[lhs, lhs, rhs])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntShl => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.left_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.signed_right_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntUnsignedShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.right_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntWrappingAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_add(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntWrappingMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_mul(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntWrappingSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_sub(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntCheckedAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let add = self.module.intrinsic(
                            "llvm.sadd.with.overflow",
                            &[self.builder.context.i64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntCheckedMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let add = self.module.intrinsic(
                            "llvm.smul.with.overflow",
                            &[self.builder.context.i64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntCheckedSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let add = self.module.intrinsic(
                            "llvm.ssub.with.overflow",
                            &[self.builder.context.i64_type().into()],
                        );

                        let res = self
                            .builder
                            .call(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::Panic => {
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_untyped_pointer(val_var);
                        let func_name = RuntimeFunction::ProcessPanic;
                        let func = self.module.runtime_function(func_name);
                        let proc = self.load_process().into();

                        self.builder.call_void(func, &[proc, val.into()]);
                        self.builder.unreachable();
                    }
                    BuiltinFunction::StringConcat => {
                        let reg_var = self.variables[&ins.register];
                        let len =
                            self.builder.i64_literal(ins.arguments.len() as _);
                        let temp_type = self
                            .builder
                            .context
                            .pointer_type()
                            .array_type(ins.arguments.len() as _);
                        let temp_var = self.builder.new_stack_slot(temp_type);

                        for (idx, reg) in ins.arguments.iter().enumerate() {
                            let var = self.variables[reg];
                            let typ = self.variable_types[reg];
                            let val = self.builder.load(typ, var);

                            self.builder.store_array_field(
                                temp_type, temp_var, idx as _, val,
                            );
                        }

                        let state = self.load_state();
                        let func_name = RuntimeFunction::StringConcat;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(
                            func,
                            &[state.into(), temp_var.into(), len.into()],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::State => {
                        let reg_var = self.variables[&ins.register];
                        let state = self.load_state();

                        self.builder.store(reg_var, state);
                    }
                    BuiltinFunction::Process => {
                        let reg_var = self.variables[&ins.register];
                        let proc = self.load_process();

                        self.builder.store(reg_var, proc);
                    }
                    BuiltinFunction::Moved => unreachable!(),
                }
            }
            Instruction::Goto(ins) => {
                self.builder.jump(all_blocks[ins.block.0]);
            }
            Instruction::Return(ins) => {
                let var = self.variables[&ins.register];
                let typ = self.variable_types[&ins.register];
                let val = self.builder.load(typ, var);

                self.builder.return_value(Some(&val));
            }
            Instruction::Branch(ins) => {
                let var = self.variables[&ins.condition];
                let val = self.builder.load_int(var);
                let status = self.builder.int_to_bool(val);

                self.builder.branch(
                    status,
                    all_blocks[ins.if_true.0],
                    all_blocks[ins.if_false.0],
                );
            }
            Instruction::Switch(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.load_int(var);
                let mut cases = Vec::with_capacity(ins.blocks.len());

                for (index, block) in ins.blocks.iter().enumerate() {
                    cases.push((
                        self.builder.u64_literal(index as u64),
                        all_blocks[block.0],
                    ));
                }

                self.builder.exhaustive_switch(val, &cases);
            }
            Instruction::Nil(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.i64_literal(0);

                self.builder.store(var, val);
            }
            Instruction::True(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.i64_literal(1);

                self.builder.store(var, val);
            }
            Instruction::False(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.i64_literal(0);

                self.builder.store(var, val);
            }
            Instruction::Int(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.i64_literal(ins.value);

                self.builder.store(var, val);
            }
            Instruction::Float(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.f64_literal(ins.value);

                self.builder.store(var, val);
            }
            Instruction::String(ins) => {
                let var = self.variables[&ins.register];
                let typ = self.variable_types[&ins.register];
                let ptr = self.module.add_string(&ins.value).as_pointer_value();
                let val = self.builder.load(typ, ptr);

                self.builder.store(var, val);
            }
            Instruction::MoveRegister(ins) => {
                let source = self.variables[&ins.source];
                let target = self.variables[&ins.target];
                let typ = self.variable_types[&ins.source];

                self.builder.store(target, self.builder.load(typ, source));
            }
            Instruction::CallExtern(ins) => {
                self.set_debug_location(ins.location);

                let func_name = ins.method.name(&self.shared.state.db);
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> =
                    Vec::with_capacity(ins.arguments.len() + 1);

                let sret = if let Some(typ) =
                    self.layouts.methods[ins.method.0 as usize].struct_return
                {
                    let var = self.builder.new_stack_slot(typ);

                    args.push(var.into());
                    Some((typ, var))
                } else {
                    None
                };

                for &reg in &ins.arguments {
                    let typ = self.variable_types[&reg];
                    let var = self.variables[&reg];

                    args.push(self.builder.load(typ, var).into());
                }

                if func.get_type().get_return_type().is_some() {
                    let var = self.variables[&ins.register];

                    self.builder.store(var, self.builder.call(func, &args));
                } else {
                    self.builder.call_void(func, &args);

                    if let Some((typ, temp)) = sret {
                        let ret = self.variables[&ins.register];

                        self.builder.store(ret, self.builder.load(typ, temp));
                    }

                    if self
                        .register_type(ins.register)
                        .is_never(&self.shared.state.db)
                    {
                        self.builder.unreachable();
                    }
                }
            }
            Instruction::CallStatic(ins) => {
                self.set_debug_location(ins.location);

                let func_name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> =
                    Vec::with_capacity(ins.arguments.len());

                for reg in &ins.arguments {
                    let var = self.variables[reg];
                    let typ = self.variable_types[reg];

                    args.push(self.builder.load(typ, var).into());
                }

                self.call(ins.register, func, &args);
            }
            Instruction::CallInstance(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let func_name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> =
                    vec![self.builder.load(rec_typ, rec_var).into()];

                for reg in &ins.arguments {
                    let typ = self.variable_types[reg];
                    let var = self.variables[reg];

                    args.push(self.builder.load(typ, var).into());
                }

                self.call(ins.register, func, &args);
            }
            Instruction::CallDynamic(ins) => {
                self.set_debug_location(ins.location);

                // For dynamic dispatch we use hashing as described in
                // https://thume.ca/2019/07/29/shenanigans-with-hash-tables/.
                //
                // Probing is only performed if collisions are known to be
                // possible for a certain hash.
                let loop_start = self.builder.add_block();
                let after_loop = self.builder.add_block();
                let idx_typ = self.builder.context.i64_type();
                let idx_var = self.builder.new_stack_slot(idx_typ);
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let rec = self.builder.load(rec_typ, rec_var);
                let info = &self.shared.methods.info[ins.method.0 as usize];
                let fn_typ =
                    self.layouts.methods[ins.method.0 as usize].signature;

                let rec_class = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        rec.into_pointer_value(),
                        HEADER_CLASS_INDEX,
                    )
                    .into_pointer_value();

                let rec_type = self.layouts.empty_class;

                // (class.method_slots - 1) as u64
                let len = self.builder.int_to_int(
                    self.builder.int_sub(
                        self.builder
                            .load_field(
                                rec_type,
                                rec_class,
                                CLASS_METHODS_COUNT_INDEX,
                            )
                            .into_int_value(),
                        self.builder.u16_literal(1),
                    ),
                    64,
                    false,
                );

                let hash = self.builder.u64_literal(info.hash);

                self.builder.store(idx_var, hash);

                let space = AddressSpace::default();
                let fn_var =
                    self.builder.new_stack_slot(fn_typ.ptr_type(space));

                self.builder.jump(loop_start);

                // The start of the probing loop (probing is necessary).
                self.builder.switch_to_block(loop_start);

                // slot = index & len
                let idx = self.builder.load(idx_typ, idx_var).into_int_value();
                let slot = self.builder.bit_and(idx, len);
                let method_addr = self.builder.array_field_index_address(
                    rec_type,
                    rec_class,
                    CLASS_METHODS_INDEX,
                    slot,
                );

                let method = self
                    .builder
                    .load(self.layouts.method, method_addr)
                    .into_struct_value();

                // We only generate the probing code when it's actually
                // necessary. In practise most dynamic dispatch call sites won't
                // need probing.
                if info.collision {
                    let ne_block = self.builder.add_block();

                    // method.hash == hash
                    let mhash = self
                        .builder
                        .extract_field(method, METHOD_HASH_INDEX)
                        .into_int_value();
                    let hash_eq = self.builder.int_eq(mhash, hash);

                    self.builder.branch(hash_eq, after_loop, ne_block);

                    // The block to jump to when the hash codes didn't match.
                    self.builder.switch_to_block(ne_block);
                    self.builder.store(
                        idx_var,
                        self.builder.int_add(idx, self.builder.u64_literal(1)),
                    );
                    self.builder.jump(loop_start);
                } else {
                    self.builder.jump(after_loop);
                }

                // The block to jump to at the end of the loop, used for
                // calling the native function.
                self.builder.switch_to_block(after_loop);
                self.builder.store(
                    fn_var,
                    self.builder.extract_field(method, METHOD_FUNCTION_INDEX),
                );

                let mut args: Vec<BasicMetadataValueEnum> = vec![rec.into()];

                for reg in &ins.arguments {
                    let typ = self.variable_types[reg];
                    let var = self.variables[reg];

                    args.push(self.builder.load(typ, var).into());
                }

                let func_val =
                    self.builder.load_function_pointer(fn_typ, fn_var);

                self.indirect_call(ins.register, fn_typ, func_val, &args);
            }
            Instruction::CallClosure(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];

                // For closures we generate the signature on the fly, as the
                // method for `call` isn't always clearly defined: for an
                // argument typed as a closure, we don't know what the actual
                // method is, thus we can't retrieve an existing signature.
                let mut sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.builder.context.pointer_type().into(), // Closure
                ];

                for reg in &ins.arguments {
                    sig_args.push(self.variable_types[reg].into());
                }

                // Load the method from the method table.
                let rec = self.builder.load(rec_typ, rec_var);
                let class = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        rec.into_pointer_value(),
                        HEADER_CLASS_INDEX,
                    )
                    .into_pointer_value();

                let mut args: Vec<BasicMetadataValueEnum> = vec![rec.into()];

                for reg in &ins.arguments {
                    let typ = self.variable_types[reg];
                    let var = self.variables[reg];

                    args.push(self.builder.load(typ, var).into());
                }

                let slot = self.builder.u32_literal(CLOSURE_CALL_INDEX);
                let method_addr = self.builder.array_field_index_address(
                    self.layouts.empty_class,
                    class,
                    CLASS_METHODS_INDEX,
                    slot,
                );

                let method = self
                    .builder
                    .load(self.layouts.method, method_addr)
                    .into_struct_value();

                let func_val = self
                    .builder
                    .extract_field(method, METHOD_FUNCTION_INDEX)
                    .into_pointer_value();

                let func_type = self
                    .builder
                    .context
                    .pointer_type()
                    .fn_type(&sig_args, false);

                self.indirect_call(ins.register, func_type, func_val, &args);
            }
            Instruction::CallDropper(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.builder.context.pointer_type().into(), // Receiver
                ];

                let rec = self.builder.load(rec_typ, rec_var);
                let class = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        rec.into_pointer_value(),
                        HEADER_CLASS_INDEX,
                    )
                    .into_pointer_value();

                let args: Vec<BasicMetadataValueEnum> = vec![rec.into()];

                let slot = self.builder.u32_literal(DROPPER_INDEX);
                let method_addr = self.builder.array_field_index_address(
                    self.layouts.empty_class,
                    class,
                    CLASS_METHODS_INDEX,
                    slot,
                );

                let method = self
                    .builder
                    .load(self.layouts.method, method_addr)
                    .into_struct_value();

                let func_val = self
                    .builder
                    .extract_field(method, METHOD_FUNCTION_INDEX)
                    .into_pointer_value();

                let func_type = self
                    .builder
                    .context
                    .pointer_type()
                    .fn_type(&sig_args, false);

                self.indirect_call(ins.register, func_type, func_val, &args);
            }
            Instruction::Send(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let method_name = &self.shared.names.methods[&ins.method];
                let method = self
                    .module
                    .add_method(method_name, ins.method)
                    .as_global_value()
                    .as_pointer_value()
                    .into();
                let len =
                    self.builder.u8_literal(ins.arguments.len() as u8).into();
                let message_new =
                    self.module.runtime_function(RuntimeFunction::MessageNew);
                let send_message = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessSendMessage);
                let message = self
                    .builder
                    .call(message_new, &[method, len])
                    .into_pointer_value();

                // The receiver doesn't need to be stored in the message, as
                // each async method sets `self` to the process running it.
                for (index, reg) in ins.arguments.iter().enumerate() {
                    let typ = self.variable_types[reg];
                    let var = self.variables[reg];
                    let val = self.builder.load(typ, var);
                    let slot = self.builder.u32_literal(index as u32);
                    let addr = self.builder.array_field_index_address(
                        self.layouts.message,
                        message,
                        MESSAGE_ARGUMENTS_INDEX,
                        slot,
                    );

                    self.builder.store(addr, val);
                }

                let state = self.load_state();
                let sender = self.load_process().into();
                let rec = self.builder.load(rec_typ, rec_var).into();

                self.builder.call_void(
                    send_message,
                    &[state.into(), sender, rec, message.into()],
                );
            }
            Instruction::GetField(ins)
                if ins.class.kind(&self.shared.state.db).is_extern() =>
            {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let layout = self.layouts.instances[ins.class.0 as usize];
                let index = ins.field.index(&self.shared.state.db) as u32;
                let field = if rec_typ.is_pointer_type() {
                    let rec = self
                        .builder
                        .load(rec_typ, rec_var)
                        .into_pointer_value();

                    self.builder.load_field(layout, rec, index)
                } else {
                    let rec =
                        self.builder.load(rec_typ, rec_var).into_struct_value();

                    self.builder.extract_field(rec, index)
                };

                self.builder.store(reg_var, field);
            }
            Instruction::SetField(ins)
                if ins.class.kind(&self.shared.state.db).is_extern() =>
            {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let layout = self.layouts.instances[ins.class.0 as usize];
                let index = ins.field.index(&self.shared.state.db) as u32;
                let val_typ = self.variable_types[&ins.value];
                let val = self.builder.load(val_typ, val_var);

                if rec_typ.is_pointer_type() {
                    let rec = self
                        .builder
                        .load(rec_typ, rec_var)
                        .into_pointer_value();

                    self.builder.store_field(layout, rec, index, val);
                } else {
                    self.builder.store_field(layout, rec_var, index, val);
                }
            }
            Instruction::GetField(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let base = if ins.class.kind(&self.shared.state.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let layout = self.layouts.instances[ins.class.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);
                let field = self.builder.load_field(
                    layout,
                    rec.into_pointer_value(),
                    index,
                );

                self.builder.store(reg_var, field);
            }
            Instruction::FieldPointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let base = if ins.class.kind(&self.shared.state.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let layout = self.layouts.instances[ins.class.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);
                let addr = self.builder.field_address(
                    layout,
                    rec.into_pointer_value(),
                    index,
                );

                self.builder.store(reg_var, addr);
            }
            Instruction::MethodPointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let func_name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let ptr = func.as_global_value().as_pointer_value();

                self.builder.store(reg_var, ptr);
            }
            Instruction::SetField(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let val_typ = self.variable_types[&ins.value];
                let base = if ins.class.kind(&self.shared.state.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let val = self.builder.load(val_typ, val_var);
                let layout = self.layouts.instances[ins.class.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);

                self.builder.store_field(
                    layout,
                    rec.into_pointer_value(),
                    index,
                    val,
                );
            }
            Instruction::CheckRefs(ins) => {
                self.set_debug_location(ins.location);

                let var = self.variables[&ins.register];
                let val = self.builder.load_untyped_pointer(var);
                let zero = self.builder.u32_literal(0);
                let header = self.layouts.header;
                let idx = HEADER_REFS_INDEX;
                let count =
                    self.builder.load_field(header, val, idx).into_int_value();

                let is_zero = self.builder.int_eq(count, zero);
                let panic_block = self.builder.add_block();
                let ok_block = self.builder.add_block();

                self.builder.branch(is_zero, ok_block, panic_block);

                // The block to jump to when the count is _not_ zero.
                self.builder.switch_to_block(panic_block);

                let proc = self.load_process();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::ReferenceCountError);

                self.builder.call_void(func, &[proc.into(), val.into()]);
                self.builder.unreachable();

                // The block to jump to when the count is zero.
                self.builder.switch_to_block(ok_block);
            }
            Instruction::Free(ins) => {
                let var = self.variables[&ins.register];
                let ptr = self.builder.load_untyped_pointer(var);
                let func = self.module.runtime_function(RuntimeFunction::Free);

                self.builder.call_void(func, &[ptr.into()]);
            }
            Instruction::Increment(ins) => {
                let reg_var = self.variables[&ins.register];
                let val = self.builder.load_untyped_pointer(reg_var);
                let one = self.builder.u32_literal(1);
                let header = self.layouts.header;
                let idx = HEADER_REFS_INDEX;
                let old =
                    self.builder.load_field(header, val, idx).into_int_value();

                let new = self.builder.int_add(old, one);

                self.builder.store_field(header, val, idx, new);
            }
            Instruction::Decrement(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.load_untyped_pointer(var);
                let header = self.layouts.header;
                let idx = HEADER_REFS_INDEX;
                let old_refs =
                    self.builder.load_field(header, val, idx).into_int_value();

                let one = self.builder.u32_literal(1);
                let new_refs = self.builder.int_sub(old_refs, one);

                self.builder.store_field(header, val, idx, new_refs);
            }
            Instruction::IncrementAtomic(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.load_untyped_pointer(var);
                let one = self.builder.u32_literal(1);
                let field = self.builder.field_address(
                    self.layouts.header,
                    val,
                    HEADER_REFS_INDEX,
                );

                self.builder.atomic_add(field, one);
            }
            Instruction::DecrementAtomic(ins) => {
                let var = self.variables[&ins.register];
                let header =
                    self.builder.load_pointer(self.layouts.header, var);
                let drop_block = all_blocks[ins.if_true.0];
                let after_block = all_blocks[ins.if_false.0];
                let one = self.builder.u32_literal(1);
                let refs = self.builder.field_address(
                    self.layouts.header,
                    header,
                    HEADER_REFS_INDEX,
                );
                let old_refs = self.builder.atomic_sub(refs, one);
                let is_zero = self.builder.int_eq(old_refs, one);

                self.builder.branch(is_zero, drop_block, after_block);
            }
            Instruction::Allocate(ins)
                if ins.class.kind(&self.shared.state.db).is_extern() =>
            {
                // Defining the alloca already reserves (uninitialised) memory,
                // so there's nothing we actually need to do here. Setting the
                // fields is done using separate instructions.
            }
            Instruction::Allocate(ins) => {
                let reg_var = self.variables[&ins.register];
                let ptr = self.allocate(ins.class).as_basic_value_enum();

                self.builder.store(reg_var, ptr);
            }
            Instruction::Spawn(ins) => {
                let reg_var = self.variables[&ins.register];
                let name = &self.shared.names.classes[&ins.class];
                let global =
                    self.module.add_class(ins.class, name).as_pointer_value();
                let class = self.builder.load_untyped_pointer(global).into();
                let proc = self.load_process().into();
                let func =
                    self.module.runtime_function(RuntimeFunction::ProcessNew);
                let ptr = self.builder.call(func, &[proc, class]);

                self.builder.store(reg_var, ptr);
            }
            Instruction::GetConstant(ins) => {
                let var = self.variables[&ins.register];
                let typ = self.variable_types[&ins.register];
                let name = &self.shared.names.constants[&ins.id];
                let global = self.module.add_constant(name).as_pointer_value();
                let value = self.builder.load(typ, global);

                self.builder.store(var, value);
            }
            Instruction::Preempt(_) => {
                let state = self.load_state();
                let data = self.process_stack_data_pointer();
                let layout = self.layouts.process_stack_data;
                let proc = self
                    .builder
                    .load_field(layout, data, STACK_DATA_PROCESS_INDEX)
                    .into_pointer_value();
                let proc_epoch = self
                    .builder
                    .load_field(layout, data, STACK_DATA_EPOCH_INDEX)
                    .into_int_value();
                let state_epoch_addr = self.builder.field_address(
                    self.layouts.state,
                    state,
                    STATE_EPOCH_INDEX,
                );
                let state_epoch =
                    self.builder.load_atomic_counter(state_epoch_addr);
                let is_eq = self.builder.int_eq(state_epoch, proc_epoch);
                let cont_block = self.builder.add_block();
                let yield_block = self.builder.add_block();

                self.builder.branch(is_eq, cont_block, yield_block);

                // The block to jump to if we need to yield back to the
                // scheduler.
                self.builder.switch_to_block(yield_block);

                let func =
                    self.module.runtime_function(RuntimeFunction::ProcessYield);

                self.builder.call_void(func, &[proc.into()]);
                self.builder.jump(cont_block);

                // The block to jump to if we can continue running.
                self.builder.switch_to_block(cont_block);
            }
            Instruction::Finish(ins) => {
                let proc = self.load_process().into();
                let terminate = self
                    .builder
                    .context
                    .bool_type()
                    .const_int(ins.terminate as _, false)
                    .into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessFinishMessage);

                self.builder.call_void(func, &[proc, terminate]);
                self.builder.unreachable();
            }
            Instruction::Cast(ins) => {
                let reg_var = self.variables[&ins.register];
                let src_var = self.variables[&ins.source];
                let src_typ = self.variable_types[&ins.source];
                let res = match (ins.from, ins.to) {
                    (CastType::Int(_, signed), CastType::Int(size, _)) => {
                        let src = self.builder.load(src_typ, src_var);

                        self.builder
                            .int_to_int(src.into_int_value(), size, signed)
                            .as_basic_value_enum()
                    }
                    (CastType::Int(_, _), CastType::Float(size)) => {
                        let src = self.builder.load(src_typ, src_var);

                        self.builder
                            .int_to_float(src.into_int_value(), size)
                            .as_basic_value_enum()
                    }
                    (
                        CastType::Int(_, _),
                        CastType::Pointer | CastType::Object,
                    ) => {
                        let src = self
                            .builder
                            .load(src_typ, src_var)
                            .into_int_value();

                        self.builder.int_to_pointer(src).as_basic_value_enum()
                    }
                    (CastType::Float(_), CastType::Int(size, _)) => {
                        let src = self.builder.load(src_typ, src_var);

                        self.float_to_int(src.into_float_value(), size)
                            .as_basic_value_enum()
                    }
                    (CastType::Float(_), CastType::Float(size)) => {
                        let src = self.builder.load(src_typ, src_var);

                        self.builder
                            .float_to_float(src.into_float_value(), size)
                            .as_basic_value_enum()
                    }
                    (
                        CastType::Pointer | CastType::Object,
                        CastType::Int(size, _),
                    ) => {
                        let src = self.builder.load(src_typ, src_var);
                        let raw = self
                            .builder
                            .pointer_to_int(src.into_pointer_value());

                        self.builder
                            .int_to_int(raw, size, false)
                            .as_basic_value_enum()
                    }
                    (CastType::Pointer, CastType::Pointer) => {
                        // Pointers are untyped at the LLVM level and instead
                        // load/stores/etc use the types, so there's nothing
                        // special we need to do in this case.
                        self.builder.load(src_typ, src_var)
                    }
                    _ => unreachable!(),
                };

                self.builder.store(reg_var, res);
            }
            Instruction::ReadPointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let reg_typ = self.variable_types[&ins.register];
                let ptr_var = self.variables[&ins.pointer];
                let ptr_typ = self.variable_types[&ins.pointer];
                let ptr =
                    self.builder.load(ptr_typ, ptr_var).into_pointer_value();
                let val = self.builder.load(reg_typ, ptr);

                self.builder.store(reg_var, val);
            }
            Instruction::WritePointer(ins) => {
                let ptr_var = self.variables[&ins.pointer];
                let ptr_typ = self.variable_types[&ins.pointer];
                let val_var = self.variables[&ins.value];
                let val_typ = self.variable_types[&ins.value];
                let val = self.builder.load(val_typ, val_var);
                let ptr =
                    self.builder.load(ptr_typ, ptr_var).into_pointer_value();

                self.builder.store(ptr, val);
            }
            Instruction::Pointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.value];

                self.builder.store(reg_var, val_var);
            }
            Instruction::Reference(_) => unreachable!(),
            Instruction::Drop(_) => unreachable!(),
        }
    }

    fn define_register_variables(&mut self) {
        for index in 0..self.method.registers.len() {
            let id = RegisterId(index as _);
            let raw = self.method.registers.value_type(id);
            let typ = self.builder.context.llvm_type(
                &self.shared.state.db,
                self.layouts,
                raw,
            );

            self.variables.insert(id, self.builder.new_temporary(typ));
            self.variable_types.insert(id, typ);
        }
    }

    fn register_type(&self, register: RegisterId) -> TypeRef {
        self.method.registers.value_type(register)
    }

    fn call(
        &self,
        register: RegisterId,
        function: FunctionValue<'ctx>,
        arguments: &[BasicMetadataValueEnum],
    ) {
        let var = self.variables[&register];

        if self.register_type(register).is_never(&self.shared.state.db) {
            self.builder.call_void(function, arguments);
            self.builder.unreachable();
        } else {
            self.builder.store(var, self.builder.call(function, arguments));
        }
    }

    fn indirect_call(
        &self,
        register: RegisterId,
        function_type: FunctionType<'ctx>,
        function: PointerValue<'ctx>,
        arguments: &[BasicMetadataValueEnum],
    ) {
        let var = self.variables[&register];

        if self.register_type(register).is_never(&self.shared.state.db) {
            self.builder.indirect_call(function_type, function, arguments);
            self.builder.unreachable();
        } else {
            self.builder.store(
                var,
                self.builder
                    .indirect_call(function_type, function, arguments)
                    .try_as_basic_value()
                    .left()
                    .unwrap(),
            );
        }
    }

    fn set_debug_location(&self, location_id: LocationId) {
        let scope = self.builder.debug_scope();
        let (line, col) = self.shared.mir.location(location_id).line_column();
        let loc = self.module.debug_builder.new_location(line, col, scope);

        self.builder.set_debug_location(loc);
    }

    fn float_to_int(
        &mut self,
        source: FloatValue<'ctx>,
        size: u32,
    ) -> IntValue<'ctx> {
        let target = match size {
            8 => self.builder.context.i8_type(),
            16 => self.builder.context.i16_type(),
            32 => self.builder.context.i32_type(),
            _ => self.builder.context.i64_type(),
        };

        let func = self.module.intrinsic(
            "llvm.fptosi.sat",
            &[target.into(), source.get_type().into()],
        );

        self.builder.call(func, &[source.into()]).into_int_value()
    }

    fn load_process(&mut self) -> PointerValue<'ctx> {
        let data = self.process_stack_data_pointer();
        let typ = self.layouts.process_stack_data;

        self.builder
            .load_field(typ, data, STACK_DATA_PROCESS_INDEX)
            .into_pointer_value()
    }

    fn process_stack_data_pointer(&mut self) -> PointerValue<'ctx> {
        let func = self.module.intrinsic(
            "llvm.read_register",
            &[self.builder.context.i64_type().into()],
        );

        let rsp_name =
            self.shared.state.config.target.stack_pointer_register_name();
        let mname = self.builder.context.inner.metadata_string(rsp_name);
        let mnode = self.builder.context.inner.metadata_node(&[mname.into()]);
        let rsp_addr =
            self.builder.call(func, &[mnode.into()]).into_int_value();
        let mask = self.load_stack_mask();
        let addr = self.builder.bit_and(rsp_addr, mask);

        self.builder.int_to_pointer(addr)
    }

    fn load_state(&mut self) -> PointerValue<'ctx> {
        let var = self.module.add_constant(STATE_GLOBAL);

        self.builder.load_pointer(self.layouts.state, var.as_pointer_value())
    }

    fn load_stack_mask(&mut self) -> IntValue<'ctx> {
        let var = self.module.add_constant(STACK_MASK_GLOBAL);

        self.builder
            .load(self.builder.context.i64_type(), var.as_pointer_value())
            .into_int_value()
    }

    fn allocate(&mut self, class: ClassId) -> PointerValue<'ctx> {
        self.builder.allocate(
            self.module,
            &self.shared.state.db,
            self.shared.names,
            class,
        )
    }
}

/// A pass for generating the entry module and method (i.e. `main()`).
pub(crate) struct GenerateMain<'a, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    layouts: &'a Layouts<'ctx>,
    methods: &'a Methods,
    names: &'a SymbolNames,
    module: &'a Module<'a, 'ctx>,
    builder: Builder<'ctx>,
}

impl<'a, 'ctx> GenerateMain<'a, 'ctx> {
    fn new(
        db: &'a Database,
        mir: &'a Mir,
        layouts: &'a Layouts<'ctx>,
        methods: &'a Methods,
        names: &'a SymbolNames,
        module: &'a Module<'a, 'ctx>,
    ) -> GenerateMain<'a, 'ctx> {
        let space = AddressSpace::default();
        let typ = module.context.i32_type().fn_type(
            &[
                module.context.i32_type().into(),
                module.context.i8_type().ptr_type(space).into(),
            ],
            false,
        );
        let function = module.add_function("main", typ, None);
        let builder = Builder::new(module.context, function);

        GenerateMain { db, mir, layouts, methods, names, module, builder }
    }

    fn run(self) {
        let space = AddressSpace::default();
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let argc_typ = self.builder.context.i32_type();
        let argv_typ = self.builder.context.i8_type().ptr_type(space);
        let argc_var = self.builder.new_temporary(argc_typ);
        let argv_var = self.builder.new_temporary(argv_typ);

        self.builder.store(argc_var, self.builder.argument(0));
        self.builder.store(argv_var, self.builder.argument(1));

        let argc = self.builder.load(argc_typ, argc_var);
        let argv = self.builder.load(argv_typ, argv_var);
        let layout = self.layouts.method_counts;
        let counts = self.builder.new_temporary(layout);

        self.set_method_count(counts, ClassId::string());
        self.set_method_count(counts, ClassId::byte_array());

        let rt_new = self.module.runtime_function(RuntimeFunction::RuntimeNew);
        let rt_start =
            self.module.runtime_function(RuntimeFunction::RuntimeStart);
        let rt_state =
            self.module.runtime_function(RuntimeFunction::RuntimeState);
        let rt_drop =
            self.module.runtime_function(RuntimeFunction::RuntimeDrop);
        let rt_stack_mask =
            self.module.runtime_function(RuntimeFunction::RuntimeStackMask);
        let runtime = self
            .builder
            .call(rt_new, &[counts.into(), argc.into(), argv.into()])
            .into_pointer_value();

        // The state is needed by various runtime functions. Because this data
        // is the same throughout the program's lifetime, we store it in a
        // global and thus remove the need to pass it as a hidden argument to
        // every Inko method.
        let state_global = self.module.add_global_pointer(STATE_GLOBAL);
        let state =
            self.builder.call(rt_state, &[runtime.into()]).into_pointer_value();

        state_global.set_initializer(
            &self
                .layouts
                .state
                .ptr_type(AddressSpace::default())
                .const_null()
                .as_basic_value_enum(),
        );

        self.builder.store(state_global.as_pointer_value(), state);

        // We need the stack size in order to get the current process. This
        // value relies on the page size, and the page size can only be reliably
        // retrieved at runtime. Not all platforms use the same size either,
        // such as ARM64 macOS which uses 16 KiB pages instead of 4 KiB.
        let stack_size_global = self
            .module
            .add_global(self.builder.context.i64_type(), STACK_MASK_GLOBAL);

        stack_size_global.set_initializer(
            &self.builder.context.i64_type().const_zero().as_basic_value_enum(),
        );

        let stack_size = self
            .builder
            .call(rt_stack_mask, &[runtime.into()])
            .into_int_value();

        self.builder.store(stack_size_global.as_pointer_value(), stack_size);

        // Allocate and store all the classes in their corresponding globals.
        // We iterate over the values here and below such that the order is
        // stable between compilations. This doesn't matter for the code that we
        // generate here, but it makes it easier to inspect the resulting
        // executable (e.g. using `objdump --disassemble`).
        for module in self.mir.modules.values() {
            let name = &self.names.setup_classes[&module.id];
            let func = self.module.add_setup_function(name);

            self.builder.call_void(func, &[]);
        }

        // Constants need to be defined in a separate pass, as they may depends
        // on the classes (e.g. array constants need the Array class to be set
        // up).
        for module in self.mir.modules.values() {
            let name = &self.names.setup_constants[&module.id];
            let func = self.module.add_setup_function(name);

            self.builder.call_void(func, &[]);
        }

        let main_class_id = self.db.main_class().unwrap();
        let main_method_id = self.db.main_method().unwrap();
        let main_class_ptr = self
            .module
            .add_global_pointer(&self.names.classes[&main_class_id])
            .as_pointer_value();

        let main_method = self
            .module
            .add_function(
                &self.names.methods[&main_method_id],
                self.module.context.void_type().fn_type(
                    &[self.layouts.context.ptr_type(space).into()],
                    false,
                ),
                None,
            )
            .as_global_value()
            .as_pointer_value();

        let main_class =
            self.builder.load_pointer(self.layouts.empty_class, main_class_ptr);

        self.builder.call_void(
            rt_start,
            &[runtime.into(), main_class.into(), main_method.into()],
        );

        // We'll only reach this code upon successfully finishing the program.
        //
        // We don't drop the classes and other data as there's no point since
        // we're exiting here. We _do_ drop the runtime in case we want to hook
        // any additional logic into that step at some point, though technically
        // this isn't necessary.
        self.builder.call_void(rt_drop, &[runtime.into()]);
        self.builder.return_value(Some(&self.builder.u32_literal(0)));
    }

    fn set_method_count(&self, counts: PointerValue<'ctx>, class: ClassId) {
        let layout = self.layouts.method_counts;
        let count = self
            .module
            .context
            .i16_type()
            .const_int(self.methods.counts[class.0 as usize] as _, false);

        self.builder.store_field(layout, counts, class.0, count);
    }
}
