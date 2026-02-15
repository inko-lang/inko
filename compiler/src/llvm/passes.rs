use crate::compiler::module_debug_path;
use crate::config::{BuildDirectories, Opt};
use crate::llvm::builder::Builder;
use crate::llvm::constants::{
    CLOSURE_CALL_INDEX, DROPPER_INDEX, FIELD_OFFSET, HEADER_REFS_INDEX,
    HEADER_TYPE_INDEX, METHOD_FUNCTION_INDEX, METHOD_HASH_INDEX,
    PROCESS_FIELD_OFFSET, STACK_DATA_EPOCH_INDEX, STACK_DATA_PROCESS_INDEX,
    STATE_EPOCH_INDEX, TYPE_METHODS_COUNT_INDEX, TYPE_METHODS_INDEX,
};
use crate::llvm::context::Context;
use crate::llvm::layouts::{
    ArgumentType, Layouts, Method as MethodLayout, ReturnType,
};
use crate::llvm::methods::Methods;
use crate::llvm::module::Module;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::mir::{
    CastType, Constant, Instruction, InstructionLocation, Method, Mir,
    RegisterId,
};
use crate::state::State;
use crate::symbol_names::{SymbolNames, STACK_MASK_GLOBAL, STATE_GLOBAL};
use crate::target::Architecture;
use blake3::{hash, Hasher};
use inkwell::attributes::AttributeLoc;
use inkwell::basic_block::BasicBlock;
use inkwell::debug_info::AsDIScope as _;
use inkwell::module::Linkage;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target,
    TargetMachine, TargetTriple,
};
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{
    ArrayValue, BasicMetadataValueEnum, BasicValue, BasicValueEnum, FloatValue,
    FunctionValue, IntValue, PointerValue,
};
use inkwell::OptimizationLevel;
use std::collections::HashMap;
use std::fs::{create_dir_all, read, write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::scope;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use types::module_name::ModuleName;
use types::{Database, Intrinsic, TypeId, TypeRef};

const NIL_VALUE: bool = false;

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
        || state.config.verify;

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
            .is_none_or(|old_ver| old_ver != new_ver)
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

    for (module, obj_path) in mir.modules.values().zip(object_paths) {
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

        for id in &module.types {
            names.push(&symbol_names.types[id]);
        }

        for id in &module.methods {
            names.push(&symbol_names.methods[id]);
        }

        names.sort();

        let mut hasher = Hasher::new();

        for name in &names {
            hasher.update(name.as_bytes());
        }

        // We include the list of inlined methods in the hash such that if this
        // changes, we flush the cache.
        let mut inlined: Vec<_> = module
            .inlined_methods
            .iter()
            .map(|id| &symbol_names.methods[id])
            .collect();

        inlined.sort();

        for name in inlined {
            hasher.update(name.as_bytes());
        }

        // The module may contain dynamic dispatch call sites. If the need for
        // probing changes, we need to update the module's code accordingly. We
        // do this by hashing the collision states of all dynamic calls in the
        // current module, such that if any of them change, so does the hash.
        for &mid in &module.methods {
            hasher.update(&methods.info[mid.0 as usize].hash.to_le_bytes());

            for block in &mir.methods.get(&mid).unwrap().body.blocks {
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
                    .is_none_or(|old_hash| old_hash != new_hash)
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

/// Compiles all the modules into object files.
///
/// The return value is a list of file paths to the generated object files.
pub(crate) fn lower_all(
    state: &mut State,
    directories: &BuildDirectories,
    mir: Mir,
    names: &SymbolNames,
) -> Result<CompileResult, String> {
    let methods = Methods::new(&state.db, &mir);

    // The object paths are generated using Blake2, and are needed in several
    // places. We generate them once here, then reuse the data by indexing this
    // Vec based on the index of the MIR module being processed.
    let obj_paths: Vec<PathBuf> = mir
        .modules
        .values()
        .map(|m| object_path(directories, m.id.name(&state.db)))
        .collect();

    check_object_cache(state, names, &methods, directories, &obj_paths, &mir)?;

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

    // The code generation optimization level to use. This is separate from the
    // optimization passes to run.
    //
    // It's unclear what the difference is between Default and Aggressive, and
    // we've not been able to measure a difference in runtime performance. Swift
    // also appears to just use Default when optimizations are enabled
    // (https://github.com/swiftlang/swift/blob/09d122af7c08e1a6e7fe76f122ddab05b0bbda59/lib/IRGen/IRGen.cpp#L929-L931),
    // so we'll assume this is good enough.
    let level = match state.config.opt {
        Opt::Debug => OptimizationLevel::None,
        _ => OptimizationLevel::Default,
    };

    // Our "queue" is just an atomic integer in the range 0..N where N is the
    // number of MIR modules. These integers are then used to index the list of
    // MIR modules, removing the need for some sort of synchronized queue.
    let queue = AtomicUsize::new(0);
    let shared = SharedState {
        state,
        mir: &mir,
        methods: &methods,
        names,
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
            } else {
                return Err("one or more LLVM threads panicked".to_string());
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

enum CallKind<'ctx> {
    Direct(FunctionValue<'ctx>),
    Indirect(FunctionType<'ctx>, PointerValue<'ctx>),
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

        // We require SSE2 for the pause() instruction, and also require it and
        // both Neon on ARM64 to allow generated code to take advantage of their
        // instructions.
        let features = match shared.state.config.target.arch {
            Architecture::Amd64 => {
                "+fxsr,+sse2,+sse3,+sse4.1,+sse4.2,+popcnt,+cx16"
            }
            Architecture::Arm64 => "+neon",
        };
        let machine = Target::from_triple(&triple)
            .unwrap()
            .create_target_machine(
                &triple,
                "",
                features,
                shared.level,
                reloc,
                model,
            )
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
            self.shared.methods,
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

        GenerateMain::new(&self.shared.state.db, self.shared.names, &main)
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

        // The LLVM pipeline to run, including passes that we must run
        // regardless of the optimization level.
        //
        // We need to scope pass names properly, otherwise we may run into
        // issues similar to https://github.com/llvm/llvm-project/issues/81128)
        //
        // The mem2reg pass is required due to how we generate LLVM IR, without
        // it we'll produce terrible results.
        let passes = if let Opt::Release = self.shared.state.config.opt {
            "function(mem2reg),default<O2>"
        } else {
            "function(mem2reg)"
        };

        module.set_data_layout(&layout);
        module.set_triple(&self.machine.get_triple());
        module
            .run_passes(passes, &self.machine, opts)
            .expect("the LLVM passes must be valid");
    }

    fn write_object_file(
        &self,
        module: &Module,
        path: PathBuf,
    ) -> Result<PathBuf, String> {
        if self.shared.state.config.write_llvm {
            let mut path = self
                .shared
                .directories
                .llvm_ir
                .join(module_debug_path(&module.name));

            path.set_extension("ll");

            if let Some(dir) = path.parent() {
                create_dir_all(dir).map_err(|e| {
                    format!("failed to create {}: {}", dir.display(), e)
                })?;
            }

            module.print_to_file(&path).map_err(|e| {
                format!("failed to write to {}: {}", path.display(), e)
            })?;
        }

        // We verify _after_ writing the IR such that one can inspect the IR in
        // the event it's invalid.
        if self.shared.state.config.verify {
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
                self.shared.mir.methods.get(method).unwrap(),
            )
            .run();
        }

        self.setup_types();
        self.setup_constants();
        self.module.debug_builder.finalize();
    }

    fn setup_types(&mut self) {
        let ctx = self.module.context;

        for &tid in &self.shared.mir.modules[self.index].types {
            let tidx = tid.0 as usize;

            if !tid.is_heap_allocated(&self.shared.state.db) {
                continue;
            }

            // Define the name of the type as a static C string.
            let type_name = tid.name(&self.shared.state.db);
            let (type_name_typ, type_name_val) =
                ctx.null_terminated_string(type_name.as_bytes());
            let type_name_global =
                self.module.add_static_global(type_name_typ, type_name_val);

            type_name_global.set_unnamed_addr(true);

            let type_name_ptr = type_name_global.as_pointer_value();
            let methods_len = self.shared.methods.counts[tidx];
            let global_name = &self.shared.names.types[&tid];
            let global_typ = self.layouts.types[tidx];
            let global = self.module.add_type(global_name, global_typ);

            // The size of the type.
            let size = ctx.i32_type().const_int(
                self.layouts
                    .target_data
                    .get_abi_size(&self.layouts.instances[tidx]),
                false,
            );

            // Populate the method slots of the type. The method slots don't
            // necessarily match the order in which they're defined, so we
            // create an initial list with dummy methods that we then overwrite.
            let mut methods =
                vec![self.layouts.method.const_zero(); methods_len];

            for method in &self.shared.mir.types[&tid].methods {
                // Static methods aren't stored in types, nor can we call them
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

                let slot = info.index as usize;
                let hash = ctx.i64_type().const_int(info.hash, false);

                methods[slot] = self
                    .layouts
                    .method
                    .const_named_struct(&[hash.into(), func.into()]);
            }

            // The number of methods the type has.
            let methods_len_val =
                ctx.i16_type().const_int(methods_len as _, false);

            global.set_initializer(&global_typ.const_named_struct(&[
                type_name_ptr.into(),
                size.into(),
                methods_len_val.into(),
                self.layouts.method.const_array(&methods).into(),
            ]));
        }
    }

    fn setup_constants(&mut self) {
        for &cid in &self.shared.mir.modules[self.index].constants {
            let name = &self.shared.names.constants[&cid];
            let cons = &self.shared.mir.constants[&cid];
            let val = self.permanent_value(self.module.context, cons);

            self.module
                .add_constant(name, val.get_type())
                .set_initializer(&val);
        }

        // We sort this list so different compilations always produce this list
        // in a consistent order, making it easier to compare the output of
        // incremental vs non-incremental builds.
        let mut strings: Vec<_> = self.module.strings.iter().collect();

        strings.sort_by_key(|p| p.0);

        for (value, global) in strings {
            global
                .set_initializer(&self.new_string(self.module.context, value));
        }
    }

    fn permanent_value(
        &mut self,
        context: &'ctx Context,
        constant: &Constant,
    ) -> BasicValueEnum<'ctx> {
        match constant {
            Constant::Int(val) => {
                context.i64_literal(*val).as_basic_value_enum()
            }
            Constant::Float(val) => {
                context.f64_literal(*val).as_basic_value_enum()
            }
            Constant::String(val) => self.new_string(context, val),
            Constant::Bool(v) => context.bool_literal(*v).as_basic_value_enum(),
            Constant::Array(vals, typ) => self.new_array(context, vals, *typ),
        }
    }

    fn new_array(
        &mut self,
        context: &'ctx Context,
        values: &[Constant],
        typ: TypeRef,
    ) -> BasicValueEnum<'ctx> {
        let tid = typ.type_id(&self.shared.state.db).unwrap();
        let tidx = tid.0 as usize;
        let arg = tid
            .type_arguments(&self.shared.state.db)
            .unwrap()
            .values()
            .next()
            .unwrap();
        let val_typ =
            context.llvm_type(&self.shared.state.db, self.layouts, arg);

        let type_layout = self.layouts.types[tidx];
        let instance_layout = self.layouts.instances[tidx];
        let type_name = &self.shared.names.types[&tid];
        let type_global = self.module.add_type(type_name, type_layout);

        // Allocate the memory for the buffer.
        let vals = values
            .iter()
            .map(|v| self.permanent_value(context, v))
            .collect::<Vec<_>>();
        let buf_typ = val_typ.array_type(vals.len() as _);
        let buf_val = unsafe {
            // Safety: Inko arrays are statically typed so the values will
            // always have the same type as `val_typ`.
            ArrayValue::new_const_array(&val_typ, &vals)
        };
        let buf_global = self.module.add_static_global(buf_typ, buf_val);

        buf_global.set_unnamed_addr(true);

        // Allocate the array itself
        let ary_val = instance_layout.const_named_struct(&[
            // Header
            context.header(self.layouts, type_global.as_pointer_value()).into(),
            // Size
            context.i64_literal(vals.len() as _).into(),
            // Capacity
            context.i64_literal(vals.len() as _).into(),
            // Buffer
            buf_global.as_pointer_value().into(),
        ]);
        let ary_global = self.module.add_global(instance_layout, "");

        ary_global.set_initializer(&ary_val);
        ary_global.set_linkage(Linkage::Private);
        ary_global.as_pointer_value().as_basic_value_enum()
    }

    fn new_string(
        &self,
        context: &'ctx Context,
        value: &str,
    ) -> BasicValueEnum<'ctx> {
        let tid = TypeId::string();
        let tidx = tid.0 as usize;
        let type_layout = self.layouts.types[tidx];
        let instance_layout = self.layouts.instances[tidx];
        let type_name = &self.shared.names.types[&tid];
        let type_global = self.module.add_type(type_name, type_layout);

        // Allocate the memory for the string's bytes.
        let (buf_typ, buf_val) =
            context.null_terminated_string(value.as_bytes());
        let buf_global = self.module.add_static_global(buf_typ, buf_val);

        buf_global.set_unnamed_addr(true);

        // Allocate the string itself.
        let str_val = instance_layout.const_named_struct(&[
            // Header
            context
                .atomic_header(self.layouts, type_global.as_pointer_value())
                .into(),
            // Size
            context.i64_literal(value.len() as _).into(),
            // Buffer
            buf_global.as_pointer_value().into(),
        ]);
        let str_global = self.module.add_global(instance_layout, "");

        str_global.set_initializer(&str_val);
        str_global.set_linkage(Linkage::Private);
        str_global.as_pointer_value().as_basic_value_enum()
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

    /// The pointer to write structs to when performing an ABI compliant
    /// structure return.
    struct_return_value: Option<(PointerValue<'ctx>, StructType<'ctx>)>,
}

impl<'shared, 'module, 'ctx> LowerMethod<'shared, 'module, 'ctx> {
    fn new(
        shared: &'shared SharedState<'shared>,
        layouts: &'ctx Layouts<'ctx>,
        module: &'module mut Module<'shared, 'ctx>,
        method: &'shared Method,
    ) -> Self {
        let name = &shared.names.methods[&method.id];
        let function = module.add_method(&shared.state.db, name, method.id);
        let builder = Builder::new(module.context, function);
        let entry_block = builder.add_block();

        builder.switch_to_block(entry_block);

        let sret = if let ReturnType::Struct(t) =
            layouts.methods[method.id.0 as usize].returns
        {
            Some((builder.argument(0).into_pointer_value(), t))
        } else {
            None
        };

        let debug_func = module.debug_builder.new_function(
            &shared.state.db,
            shared.names,
            method.id,
        );

        builder.set_debug_function(debug_func);

        LowerMethod {
            shared,
            layouts,
            method,
            module,
            builder,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
            struct_return_value: sret,
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
        self.define_register_variables();

        // When returning structs, the first argument is a pointer to write the
        // data to, instead of the receiver.
        let off = self.struct_return_value.is_some() as usize;

        for (arg, reg) in
            self.builder.arguments().skip(off).zip(self.method.arguments.iter())
        {
            let var = self.variables[reg];
            let typ = self.variable_types[reg];

            // Depending on the ABI requirements we may pass a struct in as a
            // pointer, but expect it as a value. In this case we need to load
            // the argument pointer's value into the stack slot, instead of
            // loading the argument as-is.
            if typ.is_struct_type() && arg.is_pointer_value() {
                let val = self.builder.load(typ, arg.into_pointer_value());

                self.builder.store(var, val);
            } else if typ.is_struct_type() {
                // When passing structs the argument type might be an i64 in
                // case the struct fits in a single i64. We need to use a memcpy
                // here to ensure the generated code is correct (i.e. just a
                // load with the target struct type isn't sufficient).
                self.builder.copy_value(
                    self.layouts.target_data,
                    arg,
                    var,
                    typ,
                );
            } else {
                self.builder.store(var, arg);
            }
        }

        self.method_body();
    }

    fn async_method(&mut self) {
        self.define_register_variables();

        let arg_types = self
            .method
            .arguments
            .iter()
            .skip(1)
            .map(|r| self.variable_types[r])
            .collect::<Vec<_>>();
        let args_type = self.builder.context.struct_type(&arg_types);
        let args_var =
            self.builder.new_stack_slot(self.builder.context.pointer_type());

        self.builder.store(args_var, self.builder.argument(0));

        // For async methods we don't include the receiver in the message, as
        // we can instead just read the process from the private stack data.
        let self_var = self.variables[&self.method.arguments[0]];
        let proc = self.load_process();

        self.builder.store(self_var, proc);

        // Populate the argument stack variables according to the values stored
        // in the context structure.
        let args = self.builder.load_pointer(args_var);

        for (index, reg) in self.method.arguments.iter().skip(1).enumerate() {
            let var = self.variables[reg];
            let typ = self.variable_types[reg];
            let val =
                self.builder.load_field_as(args_type, args, index as _, typ);

            self.builder.store(var, val);
        }

        // Now that the arguments are unpacked, we can deallocate the heap
        // structure passed as part of the message.
        //
        // If no arguments are passed, the data pointer is NULL.
        if !arg_types.is_empty() {
            self.builder.free(self.builder.load_pointer(args_var));
        }

        self.method_body();
    }

    fn method_body(&mut self) {
        let mut llvm_blocks = Vec::with_capacity(self.method.body.blocks.len());

        for _ in 0..self.method.body.blocks.len() {
            llvm_blocks.push(self.builder.add_block());
        }

        self.builder.jump(llvm_blocks[self.method.body.start_id.0]);

        for (idx, block) in self.method.body.blocks.iter().enumerate() {
            let llvm_block = llvm_blocks[idx];

            self.builder.switch_to_block(llvm_block);

            for ins in &block.instructions {
                self.instruction(&llvm_blocks, ins);
            }
        }
    }

    fn instruction(&mut self, all_blocks: &[BasicBlock], ins: &Instruction) {
        match ins {
            Instruction::CallBuiltin(ins) => {
                match ins.name {
                    Intrinsic::IntDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_div(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntRem => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_rem(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntBitAnd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_and(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntBitOr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_or(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntBitNot => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var);
                        let res = self.builder.bit_not(val);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntBitXor => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.bit_xor(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_eq(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntNe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_ne(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_gt(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_ge(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_le(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_lt(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_add(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_sub(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_div(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_mul(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatMod => {
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
                    Intrinsic::FloatCeil => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.ceil",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call_with_return(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatFloor => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.floor",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call_with_return(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_eq(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatToBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let res = self
                            .builder
                            .bitcast(val, self.builder.context.i64_type())
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatFromBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var);
                        let res = self
                            .builder
                            .bitcast(val, self.builder.context.f64_type())
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_gt(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_ge(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_lt(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_float(lhs_var);
                        let rhs = self.builder.load_float(rhs_var);
                        let res = self.builder.float_le(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatIsInf => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let fabs = self.module.intrinsic(
                            "llvm.fabs",
                            &[self.builder.context.f64_type().into()],
                        );

                        let pos_val = self
                            .builder
                            .call_with_return(fabs, &[val.into()])
                            .into_float_value();

                        let pos_inf = self.builder.f64_literal(f64::INFINITY);
                        let res = self.builder.float_eq(pos_val, pos_inf);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatIsNan => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let res = self.builder.float_is_nan(val);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatRound => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.round",
                            &[self.builder.context.f64_type().into()],
                        );

                        let res = self
                            .builder
                            .call_with_return(func, &[val.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::FloatPowi => {
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
                            .call_with_return(func, &[lhs.into(), rhs.into()])
                            .into_float_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntRotateLeft => {
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
                            .call_with_return(func, &[lhs, lhs, rhs])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntRotateRight => {
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
                            .call_with_return(func, &[lhs, lhs, rhs])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntShl => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.left_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.signed_right_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntUnsignedShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.right_shift(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntWrappingAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_add(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntWrappingMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_mul(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntWrappingSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_int(lhs_var);
                        let rhs = self.builder.load_int(rhs_var);
                        let res = self.builder.int_sub(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntCheckedAdd => {
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
                            .call_with_return(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntCheckedMul => {
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
                            .call_with_return(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntCheckedSub => {
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
                            .call_with_return(add, &[lhs.into(), rhs.into()])
                            .into_struct_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntSwapBytes => {
                        let reg_var = self.variables[&ins.register];
                        let val_reg = ins.arguments[0];
                        let val_var = self.variables[&val_reg];

                        // This is done such that we can use this intrinsic with
                        // different integer types.
                        let val_typ = self.variable_types[&val_reg];
                        let signed = self
                            .method
                            .registers
                            .value_type(val_reg)
                            .is_signed_int(&self.shared.state.db);
                        let val = self
                            .builder
                            .load(val_typ, val_var)
                            .into_int_value();
                        let fun =
                            self.module.intrinsic("llvm.bswap", &[val_typ]);
                        let swapped = self
                            .builder
                            .call_with_return(fun, &[val.into()])
                            .into_int_value();

                        let res = self.builder.int_to_int(swapped, 64, signed);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntAbsolute => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var);
                        let fun = self.module.intrinsic(
                            "llvm.abs",
                            &[self.builder.context.i64_type().into()],
                        );
                        let no_poison = self.builder.bool_literal(false);
                        let res = self
                            .builder
                            .call_with_return(
                                fun,
                                &[val.into(), no_poison.into()],
                            )
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntAtomicCompareSwap => {
                        let reg_var = self.variables[&ins.register];
                        let ptr_var = self.variables[&ins.arguments[0]];
                        let old_var = self.variables[&ins.arguments[1]];
                        let old_typ = self.variable_types[&ins.arguments[1]];
                        let new_var = self.variables[&ins.arguments[2]];
                        let new_typ = self.variable_types[&ins.arguments[2]];
                        let ptr = self.builder.load_pointer(ptr_var);
                        let old = self.builder.load(old_typ, old_var);
                        let new = self.builder.load(new_typ, new_var);
                        let res = self.builder.atomic_swap(ptr, old, new);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntAtomicLoad => {
                        let reg_var = self.variables[&ins.register];
                        let reg_typ = self.variable_types[&ins.register];
                        let ptr_var = self.variables[&ins.arguments[0]];
                        let ptr = self.builder.load_pointer(ptr_var);
                        let res = self.builder.atomic_load(
                            self.layouts.target_data,
                            reg_typ,
                            ptr,
                        );

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntAtomicStore => {
                        let reg_var = self.variables[&ins.register];
                        let ptr_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let val_typ = self.variable_types[&ins.arguments[1]];
                        let nil = self.builder.bool_literal(NIL_VALUE);
                        let ptr = self.builder.load_pointer(ptr_var);
                        let val = self
                            .builder
                            .load(val_typ, val_var)
                            .into_int_value();

                        self.builder.atomic_store(
                            self.layouts.target_data,
                            ptr,
                            val,
                        );
                        self.builder.store(reg_var, nil);
                    }
                    Intrinsic::IntAtomicAdd => {
                        let reg_var = self.variables[&ins.register];
                        let ptr_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let val_typ = self.variable_types[&ins.arguments[1]];
                        let ptr = self.builder.load_pointer(ptr_var);
                        let val = self
                            .builder
                            .load(val_typ, val_var)
                            .into_int_value();

                        let res = self.builder.atomic_add(ptr, val);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntAtomicSub => {
                        let reg_var = self.variables[&ins.register];
                        let ptr_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let val_typ = self.variable_types[&ins.arguments[1]];
                        let ptr = self.builder.load_pointer(ptr_var);
                        let val = self
                            .builder
                            .load(val_typ, val_var)
                            .into_int_value();

                        let res = self.builder.atomic_sub(ptr, val);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::State => {
                        let reg_var = self.variables[&ins.register];
                        let state = self.load_state();

                        self.builder.store(reg_var, state);
                    }
                    Intrinsic::Process => {
                        let reg_var = self.variables[&ins.register];
                        let proc = self.load_process();

                        self.builder.store(reg_var, proc);
                    }
                    Intrinsic::SpinLoopHint => {
                        let reg_var = self.variables[&ins.register];
                        let nil = self.builder.bool_literal(NIL_VALUE);

                        match self.shared.state.config.target.arch {
                            Architecture::Amd64 => {
                                let func = self
                                    .module
                                    .intrinsic("llvm.x86.sse2.pause", &[]);
                                self.builder.direct_call(func, &[]);
                            }
                            Architecture::Arm64 => {
                                // For ARM64 we use the same approach as Rust by
                                // using the ISB SY instruction.
                                let sy = self.builder.u32_literal(15);
                                let func = self
                                    .module
                                    .intrinsic("llvm.aarch64.isb", &[]);

                                self.builder.direct_call(func, &[sy.into()]);
                            }
                        };

                        self.builder.store(reg_var, nil)
                    }
                    Intrinsic::BoolEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_bool(lhs_var);
                        let rhs = self.builder.load_bool(rhs_var);
                        let res = self.builder.int_eq(lhs, rhs);

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntLeadingZeros => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var).into();
                        let func = self.module.intrinsic(
                            "llvm.ctlz",
                            &[self.builder.context.i64_type().into()],
                        );
                        let no_poison = self.builder.bool_literal(false).into();
                        let res = self
                            .builder
                            .call_with_return(func, &[val, no_poison])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::IntTrailingZeros => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_int(val_var).into();
                        let func = self.module.intrinsic(
                            "llvm.cttz",
                            &[self.builder.context.i64_type().into()],
                        );
                        let no_poison = self.builder.bool_literal(false).into();
                        let res = self
                            .builder
                            .call_with_return(func, &[val, no_poison])
                            .into_int_value();

                        self.builder.store(reg_var, res);
                    }
                    Intrinsic::Moved
                    | Intrinsic::RefMove
                    | Intrinsic::MutMove => unreachable!(),
                }
            }
            Instruction::Goto(ins) => {
                self.builder.jump(all_blocks[ins.block.0]);
            }
            Instruction::Return(ins) => {
                let typ = self.variable_types[&ins.register];
                let var = self.variables[&ins.register];
                let val = self.builder.load(typ, var);

                if let Some((ptr, to_typ)) = self.struct_return_value {
                    self.builder.copy_value(
                        self.layouts.target_data,
                        val,
                        ptr,
                        to_typ.as_basic_type_enum(),
                    );
                    self.builder.return_value(None);
                } else {
                    // When returning a struct on the stack, the return type
                    // will be structurally compatible but might be nominally
                    // different.
                    //
                    // For example, if the struct is `{ i64 }` we may
                    // in fact return a value of type `i64`. While both have the
                    // same layout, they're not compatible at the LLVM level.
                    let to_typ = self
                        .builder
                        .function
                        .get_type()
                        .get_return_type()
                        .unwrap();
                    let tmp = self.builder.new_stack_slot(to_typ);

                    self.builder.copy_value(
                        self.layouts.target_data,
                        val,
                        tmp,
                        to_typ,
                    );

                    let ret = self.builder.load(to_typ, tmp);

                    self.builder.return_value(Some(&ret));
                }
            }
            Instruction::Branch(ins) => {
                let var = self.variables[&ins.condition];
                let val = self.builder.load_bool(var);

                self.builder.branch(
                    val,
                    all_blocks[ins.if_true.0],
                    all_blocks[ins.if_false.0],
                );
            }
            Instruction::Switch(ins) => {
                let reg_var = self.variables[&ins.register];
                let reg_typ = self.variable_types[&ins.register];
                let bits = reg_typ.into_int_type().get_bit_width();
                let reg_val =
                    self.builder.load(reg_typ, reg_var).into_int_value();
                let mut cases = Vec::with_capacity(ins.blocks.len());

                for &(val, block) in &ins.blocks {
                    cases.push((
                        self.builder.int_literal(bits, val as u64),
                        all_blocks[block.0],
                    ));
                }

                // We use unwrap_or_else() instead of unwrap_or() because
                // `cases` might be empty while a fallback is present, and we
                // don't want to panic in such a case.
                let fallback = ins
                    .fallback
                    .map(|b| all_blocks[b.0])
                    .unwrap_or_else(|| cases[0].1);

                self.builder.switch(reg_val, &cases, fallback);
            }
            Instruction::Nil(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.bool_literal(NIL_VALUE);

                self.builder.store(var, val);
            }
            Instruction::Bool(ins) => {
                let var = self.variables[&ins.register];
                let val = self.builder.bool_literal(ins.value);

                self.builder.store(var, val);
            }
            Instruction::Int(ins) => {
                let var = self.variables[&ins.register];
                let val =
                    self.builder.int_literal(ins.bits as _, ins.value as u64);

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

                let name = ins.method.name(&self.shared.state.db);
                let fn_val = self.module.add_method(
                    &self.shared.state.db,
                    name,
                    ins.method,
                );
                let kind = CallKind::Direct(fn_val);
                let layout = &self.layouts.methods[ins.method.0 as usize];

                self.call(kind, layout, ins.register, None, &ins.arguments)
            }
            Instruction::CallStatic(ins) => {
                self.set_debug_location(ins.location);

                let func_name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(
                    &self.shared.state.db,
                    func_name,
                    ins.method,
                );
                let kind = CallKind::Direct(func);
                let layout = &self.layouts.methods[ins.method.0 as usize];

                self.call(kind, layout, ins.register, None, &ins.arguments);
            }
            Instruction::CallInstance(ins) => {
                self.set_debug_location(ins.location);

                let name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(
                    &self.shared.state.db,
                    name,
                    ins.method,
                );
                let kind = CallKind::Direct(func);
                let layout = &self.layouts.methods[ins.method.0 as usize];

                self.call(
                    kind,
                    layout,
                    ins.register,
                    Some(ins.receiver),
                    &ins.arguments,
                );
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
                let layout = &self.layouts.methods[ins.method.0 as usize];
                let fn_typ = layout.signature(self.builder.context);
                let rec_type_ptr = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        rec.into_pointer_value(),
                        HEADER_TYPE_INDEX,
                    )
                    .into_pointer_value();

                let rec_type = self.layouts.empty_type;

                // (type.method_slots - 1) as u64
                let len = self.builder.int_to_int(
                    self.builder.int_sub(
                        self.builder
                            .load_field(
                                rec_type,
                                rec_type_ptr,
                                TYPE_METHODS_COUNT_INDEX,
                            )
                            .into_int_value(),
                        self.builder.u16_literal(1),
                    ),
                    64,
                    false,
                );

                let hash = self.builder.u64_literal(info.hash);

                self.builder.store(idx_var, hash);

                let fn_var = self
                    .builder
                    .new_stack_slot(self.builder.context.pointer_type());

                self.builder.jump(loop_start);

                // The start of the probing loop (probing is necessary).
                self.builder.switch_to_block(loop_start);

                // slot = index & len
                let idx = self.builder.load(idx_typ, idx_var).into_int_value();
                let slot = self.builder.bit_and(idx, len);
                let method_addr = self.builder.array_field_index_address(
                    rec_type,
                    rec_type_ptr,
                    TYPE_METHODS_INDEX,
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
                let fn_val = self.builder.load_pointer(fn_var);
                let kind = CallKind::Indirect(fn_typ, fn_val);

                self.call(
                    kind,
                    layout,
                    ins.register,
                    Some(ins.receiver),
                    &ins.arguments,
                );
            }
            Instruction::CallClosure(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let reg_typ = self.variable_types[&ins.register];

                // For closures we generate the signature on the fly, as the
                // method for `call` isn't always clearly defined: for an
                // argument typed as a closure, we don't know what the actual
                // method is, thus we can't retrieve an existing signature.
                let mut layout = MethodLayout::new();

                layout.returns = self.builder.context.return_type(
                    self.shared.state,
                    self.layouts.target_data,
                    reg_typ,
                );

                if let ReturnType::Struct(t) = layout.returns {
                    layout.arguments.push(ArgumentType::StructReturn(t))
                }

                for &reg in [ins.receiver].iter().chain(ins.arguments.iter()) {
                    let typ = self.builder.context.argument_type(
                        self.shared.state,
                        self.layouts,
                        self.register_type(reg),
                    );

                    layout.arguments.push(typ);
                }

                // Load the method from the method table.
                let rec =
                    self.builder.load(rec_typ, rec_var).into_pointer_value();
                let typ_ptr = self
                    .builder
                    .load_field(self.layouts.header, rec, HEADER_TYPE_INDEX)
                    .into_pointer_value();
                let slot = self.builder.u32_literal(CLOSURE_CALL_INDEX);
                let method_addr = self.builder.array_field_index_address(
                    self.layouts.empty_type,
                    typ_ptr,
                    TYPE_METHODS_INDEX,
                    slot,
                );

                let method = self
                    .builder
                    .load(self.layouts.method, method_addr)
                    .into_struct_value();
                let fn_val = self
                    .builder
                    .extract_field(method, METHOD_FUNCTION_INDEX)
                    .into_pointer_value();
                let fn_type = layout.signature(self.builder.context);
                let kind = CallKind::Indirect(fn_type, fn_val);

                self.call(
                    kind,
                    &layout,
                    ins.register,
                    Some(ins.receiver),
                    &ins.arguments,
                );
            }
            Instruction::CallDropper(ins) => {
                self.set_debug_location(ins.location);

                let reg_typ = self.variable_types[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let mut layout = MethodLayout::new();

                layout.returns = ReturnType::Regular(reg_typ);
                layout.arguments.push(self.builder.context.argument_type(
                    self.shared.state,
                    self.layouts,
                    self.register_type(ins.receiver),
                ));

                let rec =
                    self.builder.load(rec_typ, rec_var).into_pointer_value();
                let rec_type_ptr = self
                    .builder
                    .load_field(self.layouts.header, rec, HEADER_TYPE_INDEX)
                    .into_pointer_value();
                let slot = self.builder.u32_literal(DROPPER_INDEX);
                let addr = self.builder.array_field_index_address(
                    self.layouts.empty_type,
                    rec_type_ptr,
                    TYPE_METHODS_INDEX,
                    slot,
                );
                let method = self
                    .builder
                    .load(self.layouts.method, addr)
                    .into_struct_value();
                let fn_val = self
                    .builder
                    .extract_field(method, METHOD_FUNCTION_INDEX)
                    .into_pointer_value();
                let fn_typ = layout.signature(self.builder.context);
                let kind = CallKind::Indirect(fn_typ, fn_val);

                self.call(kind, &layout, ins.register, Some(ins.receiver), &[]);
            }
            Instruction::Send(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let method_name = &self.shared.names.methods[&ins.method];
                let method = self
                    .module
                    .add_method(&self.shared.state.db, method_name, ins.method)
                    .as_global_value()
                    .as_pointer_value()
                    .into();
                let send_message = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessSendMessage);
                let arg_types = ins
                    .arguments
                    .iter()
                    .map(|r| self.variable_types[r])
                    .collect::<Vec<_>>();
                let args = if arg_types.is_empty() {
                    self.builder.context.pointer_type().const_null()
                } else {
                    let args_type =
                        self.builder.context.struct_type(&arg_types);
                    let args = self.builder.malloc(self.module, args_type);

                    // The receiver doesn't need to be stored in the message, as
                    // each async method sets `self` to the process running it.
                    for (index, reg) in ins.arguments.iter().enumerate() {
                        let typ = self.variable_types[reg];
                        let var = self.variables[reg];
                        let val = self.builder.load(typ, var);

                        self.builder
                            .store_field(args_type, args, index as _, val);
                    }

                    args
                };

                let sender = self.load_process().into();
                let rec = self.builder.load(rec_typ, rec_var).into();

                self.builder.direct_call(
                    send_message,
                    &[sender, rec, method, args.into()],
                );
            }
            Instruction::GetField(ins)
                if ins.type_id.is_heap_allocated(&self.shared.state.db) =>
            {
                let reg_var = self.variables[&ins.register];
                let reg_typ = self.variable_types[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let tkind = ins.type_id.kind(&self.shared.state.db);
                let base = if tkind.is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let layout = self.layouts.instances[ins.type_id.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);

                // When loading fields from enums we may load from an opaque
                // field type, depending on what constructor we're dealing with.
                // To ensure we always use the correct type, we use the type of
                // the return value instead of using the layout's field type
                // as-is.
                let field = self.builder.load_field_as(
                    layout,
                    rec.into_pointer_value(),
                    index,
                    reg_typ,
                );

                self.builder.store(reg_var, field);
            }
            Instruction::GetField(ins) => {
                let reg_var = self.variables[&ins.register];
                let reg_typ = self.variable_types[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let layout = self.layouts.instances[ins.type_id.0 as usize];
                let index = ins.field.index(&self.shared.state.db) as u32;
                let field = if rec_typ.is_pointer_type() {
                    let rec = self
                        .builder
                        .load(rec_typ, rec_var)
                        .into_pointer_value();

                    self.builder.load_field_as(layout, rec, index, reg_typ)
                } else {
                    // We don't use extractvalue because the type of the field
                    // may not match that of the target register (e.g. when
                    // loading an enum constructor field). Using getelementptr
                    // plus a load allows us to perform a load using a specific
                    // type.
                    self.builder.load_field_as(layout, rec_var, index, reg_typ)
                };

                self.builder.store(reg_var, field);
            }
            Instruction::SetField(ins)
                if ins.type_id.is_heap_allocated(&self.shared.state.db) =>
            {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let val_typ = self.variable_types[&ins.value];
                let base = if ins.type_id.kind(&self.shared.state.db).is_async()
                {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let val = self.builder.load(val_typ, val_var);
                let layout = self.layouts.instances[ins.type_id.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);

                self.builder.store_field(
                    layout,
                    rec.into_pointer_value(),
                    index,
                    val,
                );
            }
            Instruction::SetField(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let layout = self.layouts.instances[ins.type_id.0 as usize];
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
            Instruction::FieldPointer(ins)
                if ins.type_id.is_heap_allocated(&self.shared.state.db) =>
            {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let base = if ins.type_id.kind(&self.shared.state.db).is_async()
                {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index =
                    (base + ins.field.index(&self.shared.state.db)) as u32;
                let layout = self.layouts.instances[ins.type_id.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);
                let addr = self.builder.field_address(
                    layout,
                    rec.into_pointer_value(),
                    index,
                );

                self.builder.store(reg_var, addr);
            }
            Instruction::FieldPointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let layout = self.layouts.instances[ins.type_id.0 as usize];
                let rec = self.builder.load(rec_typ, rec_var);
                let index = ins.field.index(&self.shared.state.db) as u32;
                let src = if rec_typ.is_pointer_type() {
                    rec.into_pointer_value()
                } else {
                    rec_var
                };
                let addr = self.builder.field_address(layout, src, index);

                self.builder.store(reg_var, addr);
            }
            Instruction::MethodPointer(ins) => {
                let reg_var = self.variables[&ins.register];
                let func_name = &self.shared.names.methods[&ins.method];
                let func = self.module.add_method(
                    &self.shared.state.db,
                    func_name,
                    ins.method,
                );
                let ptr = func.as_global_value().as_pointer_value();

                self.builder.store(reg_var, ptr);
            }
            Instruction::CheckRefs(ins) => {
                self.set_debug_location(ins.location);

                let var = self.variables[&ins.register];
                let val = self.builder.load_pointer(var);
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

                self.builder.direct_call(func, &[proc.into(), val.into()]);
                self.builder.unreachable();

                // The block to jump to when the count is zero.
                self.builder.switch_to_block(ok_block);
            }
            Instruction::Free(ins) => {
                let var = self.variables[&ins.register];
                let ptr = self.builder.load_pointer(var);
                let func = self.module.runtime_function(RuntimeFunction::Free);

                self.builder.direct_call(func, &[ptr.into()]);
            }
            Instruction::Increment(ins) => {
                let reg_var = self.variables[&ins.register];
                let val = self.builder.load_pointer(reg_var);
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
                let val = self.builder.load_pointer(var);
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
                let val = self.builder.load_pointer(var);
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
                let header = self.builder.load_pointer(var);
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
                if ins.type_id.is_stack_allocated(&self.shared.state.db) =>
            {
                // Defining the alloca already reserves (uninitialised) memory,
                // so there's nothing we actually need to do here. Setting the
                // fields is done using separate instructions.
            }
            Instruction::Allocate(ins) => {
                self.set_debug_location(ins.location);

                let reg_var = self.variables[&ins.register];
                let ptr = self.allocate(ins.type_id).as_basic_value_enum();

                self.builder.store(reg_var, ptr);
            }
            Instruction::Spawn(ins) => {
                self.set_debug_location(ins.location);

                let tid = ins.type_id;
                let reg_var = self.variables[&ins.register];
                let name = &self.shared.names.types[&tid];
                let proc_type = self
                    .module
                    .add_type(name, self.layouts.types[tid.0 as usize])
                    .as_pointer_value()
                    .into();
                let func =
                    self.module.runtime_function(RuntimeFunction::ProcessNew);
                let ptr = self.builder.call_with_return(func, &[proc_type]);

                self.builder.store(reg_var, ptr);
            }
            Instruction::GetConstant(ins) => {
                let var = self.variables[&ins.register];
                let typ = self.variable_types[&ins.register];
                let name = &self.shared.names.constants[&ins.id];
                let global =
                    self.module.add_constant(name, typ).as_pointer_value();
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

                self.builder.direct_call(func, &[proc.into()]);
                self.builder.jump(cont_block);

                // The block to jump to if we can continue running.
                self.builder.switch_to_block(cont_block);
            }
            Instruction::Finish(ins) => {
                let proc = self.load_process().into();
                let terminate = self.builder.bool_literal(ins.terminate).into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessFinishMessage);

                self.builder.direct_call(func, &[proc, terminate]);
                self.builder.unreachable();
            }
            Instruction::Cast(ins) => {
                let reg_var = self.variables[&ins.register];
                let src_var = self.variables[&ins.source];
                let src_typ = self.variable_types[&ins.source];
                let res = match (ins.from, ins.to) {
                    (CastType::Int(_, sign), CastType::Int(size, _)) => {
                        let src = self.builder.load(src_typ, src_var);
                        let signed = sign.is_signed();

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
                    // Only heap allocated values can be cast to a trait, and
                    // there's nothing special to do for such cases.
                    (_, CastType::Trait) => self.builder.load(src_typ, src_var),
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
            Instruction::SizeOf(ins) => {
                let reg_var = self.variables[&ins.register];
                let typ = self.builder.context.llvm_type(
                    &self.shared.state.db,
                    self.layouts,
                    ins.argument,
                );

                self.builder.store(reg_var, typ.size_of().unwrap());
            }
            Instruction::Borrow(_) => unreachable!(),
            Instruction::Drop(_) => unreachable!(),
        }
    }

    fn define_register_variables(&mut self) {
        let mut zero = Vec::new();

        for index in 0..self.method.registers.len() {
            let reg = RegisterId(index as _);
            let reg_typ = self.register_type(reg);
            let typ = self.builder.context.llvm_type(
                &self.shared.state.db,
                self.layouts,
                reg_typ,
            );

            let alloca = self.builder.new_temporary(typ);

            // Extern types are zeroed out by default because we allow partial
            // initialization to make working with C easier.
            if reg_typ.is_extern_instance(&self.shared.state.db) {
                zero.push((alloca, typ));
            }

            self.variables.insert(reg, alloca);
            self.variable_types.insert(reg, typ);
        }

        for (alloca, typ) in zero {
            self.builder.store(alloca, typ.const_zero());
        }
    }

    fn register_type(&self, register: RegisterId) -> TypeRef {
        self.method.registers.value_type(register)
    }

    fn call(
        &self,
        kind: CallKind<'ctx>,
        layout: &MethodLayout<'ctx>,
        register: RegisterId,
        receiver: Option<RegisterId>,
        arguments: &[RegisterId],
    ) {
        let mut args: Vec<BasicMetadataValueEnum> = Vec::with_capacity(
            arguments.len()
                + receiver.is_some() as usize
                + layout.returns.is_struct() as usize,
        );

        // When using struct returns, the returned data is written to a pointer
        // which we then read into the desired return register _after_ the call.
        let mut attrs = Vec::new();
        let sret = if let ReturnType::Struct(typ) = layout.returns {
            let var = self.builder.new_stack_slot(typ);

            attrs.push((
                AttributeLoc::Param(0),
                self.builder.context.type_attribute("sret", typ.into()),
            ));
            args.push(var.into());
            Some((typ, var))
        } else {
            None
        };

        for (idx, reg) in receiver.iter().chain(arguments.iter()).enumerate() {
            let idx = if sret.is_some() { idx + 1 } else { idx };
            let var = self.variables[reg];
            let typ = self.variable_types[reg];

            match layout.arguments.get(idx).cloned() {
                Some(ArgumentType::Regular(t)) => {
                    let raw_val = self.builder.load(typ, var);
                    let val = if t != typ {
                        let tmp = self.builder.new_stack_slot(t);

                        self.builder.copy_value(
                            self.layouts.target_data,
                            raw_val,
                            tmp,
                            t,
                        );
                        self.builder.load(t, tmp)
                    } else {
                        raw_val
                    };

                    args.push(val.into());
                }
                Some(ArgumentType::StructValue(t)) => {
                    attrs.push((
                        AttributeLoc::Param(idx as u32),
                        self.builder.context.type_attribute("byval", t.into()),
                    ));

                    args.push(var.into());
                }
                Some(ArgumentType::StructReturn(_)) => {
                    // We only iterate over explicitly provided arguments and
                    // those don't include the sret pointer. In addition, we
                    // handle sret arguments before the iteration, so there's
                    // nothing we need to do here.
                }
                Some(ArgumentType::Pointer) => {
                    args.push(var.into());
                }
                None => {
                    // We may run into this case when calling a variadic
                    // function and passing more arguments than are defined.
                    args.push(self.builder.load(typ, var).into());
                }
            }
        }

        let reg_var = self.variables[&register];
        let reg_typ = self.variable_types[&register];
        let call_site = match kind {
            CallKind::Direct(f) => self.builder.direct_call(f, &args),
            CallKind::Indirect(t, f) => self.builder.indirect_call(t, f, &args),
        };

        for (loc, attr) in attrs {
            call_site.add_attribute(loc, attr);
        }

        if layout.returns.is_regular() {
            let val = call_site.try_as_basic_value().left().unwrap();

            self.builder.copy_value(
                self.layouts.target_data,
                val,
                reg_var,
                reg_typ,
            );
        } else if let Some((typ, tmp)) = sret {
            let val = self.builder.load(typ, tmp);

            self.builder.copy_value(
                self.layouts.target_data,
                val,
                reg_var,
                reg_typ,
            );
        }

        if self.register_type(register).is_never(&self.shared.state.db) {
            self.builder.unreachable();
        }
    }

    fn set_debug_location(&mut self, location: InstructionLocation) {
        let line = location.line;
        let col = location.column;
        let loc = if let Some(id) = location.inlined_call_id() {
            let chain = &self.method.inlined_calls[id];
            let mut parent = None;

            // We process the list in reverse order such that the first entry we
            // process is the outer-most call in the chain (since we inline
            // in bottom-up order).
            for call in chain.chain.iter().rev() {
                let line = call.location.line;
                let col = call.location.column;
                let loc = if let Some(parent) = parent {
                    let scope = self
                        .module
                        .debug_builder
                        .new_function(
                            &self.shared.state.db,
                            self.shared.names,
                            call.caller,
                        )
                        .as_debug_info_scope();

                    self.module
                        .debug_builder
                        .new_inlined_location(line, col, scope, parent)
                } else {
                    let scope = self.builder.debug_scope();

                    self.module.debug_builder.new_location(line, col, scope)
                };

                parent = Some(loc);
            }

            let parent = parent.unwrap();
            let scope = self
                .module
                .debug_builder
                .new_function(
                    &self.shared.state.db,
                    self.shared.names,
                    chain.source_method,
                )
                .as_debug_info_scope();

            self.module
                .debug_builder
                .new_inlined_location(line, col, scope, parent)
        } else {
            let scope = self.builder.debug_scope();

            self.module.debug_builder.new_location(line, col, scope)
        };

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

        self.builder.call_with_return(func, &[source.into()]).into_int_value()
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
        let rsp_addr = self
            .builder
            .call_with_return(func, &[mnode.into()])
            .into_int_value();
        let mask = self.load_stack_mask();
        let addr = self.builder.bit_and(rsp_addr, mask);

        self.builder.int_to_pointer(addr)
    }

    fn load_state(&mut self) -> PointerValue<'ctx> {
        let var = self
            .module
            .add_constant(STATE_GLOBAL, self.builder.context.pointer_type())
            .as_pointer_value();

        self.builder.load_pointer(var)
    }

    fn load_stack_mask(&mut self) -> IntValue<'ctx> {
        let typ = self.builder.context.i64_type();
        let var =
            self.module.add_constant(STACK_MASK_GLOBAL, typ).as_pointer_value();

        self.builder.load(typ, var).into_int_value()
    }

    fn allocate(&mut self, type_id: TypeId) -> PointerValue<'ctx> {
        self.builder.allocate_instance(
            self.module,
            &self.shared.state.db,
            self.shared.names,
            type_id,
        )
    }
}

/// A pass for generating the entry module and method (i.e. `main()`).
pub(crate) struct GenerateMain<'a, 'ctx> {
    db: &'a Database,
    names: &'a SymbolNames,
    module: &'a Module<'a, 'ctx>,
    builder: Builder<'ctx>,
}

impl<'a, 'ctx> GenerateMain<'a, 'ctx> {
    fn new(
        db: &'a Database,
        names: &'a SymbolNames,
        module: &'a Module<'a, 'ctx>,
    ) -> GenerateMain<'a, 'ctx> {
        let typ = module.context.i32_type().fn_type(
            &[
                module.context.i32_type().into(),
                module.context.pointer_type().into(),
            ],
            false,
        );
        let function = module.add_function("main", typ, None);
        let builder = Builder::new(module.context, function);

        GenerateMain { db, names, module, builder }
    }

    fn run(self) {
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let argc_typ = self.builder.context.i32_type();
        let argv_typ = self.builder.context.pointer_type();
        let argc_var = self.builder.new_temporary(argc_typ);
        let argv_var = self.builder.new_temporary(argv_typ);

        self.builder.store(argc_var, self.builder.argument(0));
        self.builder.store(argv_var, self.builder.argument(1));

        let argc = self.builder.load(argc_typ, argc_var);
        let argv = self.builder.load(argv_typ, argv_var);

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
            .call_with_return(rt_new, &[argc.into(), argv.into()])
            .into_pointer_value();

        // The state is needed by various runtime functions. Because this data
        // is the same throughout the program's lifetime, we store it in a
        // global and thus remove the need to pass it as a hidden argument to
        // every Inko method.
        let state_global = self.module.add_global_pointer(STATE_GLOBAL);
        let state = self
            .builder
            .call_with_return(rt_state, &[runtime.into()])
            .into_pointer_value();

        state_global.set_initializer(
            &self
                .builder
                .context
                .pointer_type()
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
            .call_with_return(rt_stack_mask, &[runtime.into()])
            .into_int_value();

        self.builder.store(stack_size_global.as_pointer_value(), stack_size);

        let main_tid = self.db.main_type().unwrap();
        let main_method_id = self.db.main_method().unwrap();
        let main_type = self
            .module
            .add_global_pointer(&self.names.types[&main_tid])
            .as_pointer_value();

        let main_method = self
            .module
            .add_function(
                &self.names.methods[&main_method_id],
                self.module.context.void_type().fn_type(
                    &[self.module.context.pointer_type().into()],
                    false,
                ),
                None,
            )
            .as_global_value()
            .as_pointer_value();

        self.builder.direct_call(
            rt_start,
            &[runtime.into(), main_type.into(), main_method.into()],
        );

        // We'll only reach this code upon successfully finishing the program.
        //
        // We don't drop the types and other data as there's no point since
        // we're exiting here. We _do_ drop the runtime in case we want to hook
        // any additional logic into that step at some point, though technically
        // this isn't necessary.
        self.builder.direct_call(rt_drop, &[runtime.into()]);
        self.builder.return_value(Some(&self.builder.u32_literal(0)));
    }
}
