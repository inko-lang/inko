use crate::config::BuildDirectories;
use crate::llvm::builder::Builder;
use crate::llvm::constants::{
    ATOMIC_KIND, BOXED_FLOAT_VALUE_INDEX, BOXED_INT_VALUE_INDEX,
    CLASS_METHODS_COUNT_INDEX, CLASS_METHODS_INDEX, CLOSURE_CALL_INDEX,
    CONTEXT_ARGS_INDEX, CONTEXT_PROCESS_INDEX, CONTEXT_STATE_INDEX,
    DROPPER_INDEX, FALSE_INDEX, FIELD_OFFSET, FLOAT_KIND, HASH_KEY0_INDEX,
    HASH_KEY1_INDEX, HEADER_CLASS_INDEX, HEADER_KIND_INDEX, HEADER_REFS_INDEX,
    INT_KIND, INT_MASK, INT_SHIFT, LLVM_RESULT_STATUS_INDEX,
    LLVM_RESULT_VALUE_INDEX, MAX_INT, MESSAGE_ARGUMENTS_INDEX,
    METHOD_FUNCTION_INDEX, METHOD_HASH_INDEX, MIN_INT, NIL_INDEX, OWNED_KIND,
    PERMANENT_KIND, PROCESS_FIELD_OFFSET, REF_KIND, REF_MASK, TAG_MASK,
    TRUE_INDEX,
};
use crate::llvm::context::Context;
use crate::llvm::layouts::Layouts;
use crate::llvm::module::Module;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::mir::{
    CloneKind, Constant, Instruction, LocationId, Method, Mir, RegisterId,
};
use crate::state::State;
use crate::symbol_names::SymbolNames;
use crate::target::Architecture;
use inkwell::basic_block::BasicBlock;
use inkwell::passes::{PassManager, PassManagerBuilder};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple,
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
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use types::module_name::ModuleName;
use types::{BuiltinFunction, ClassId, Database};

/// A compiler pass that compiles Inko MIR into object files using LLVM.
pub(crate) struct Compile<'a, 'b, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    module_index: usize,
    layouts: &'a Layouts<'ctx>,
    names: &'a SymbolNames,
    context: &'ctx Context,
    module: &'b mut Module<'a, 'ctx>,

    /// All native functions and the class IDs they belong to.
    functions: HashMap<ClassId, Vec<FunctionValue<'ctx>>>,
}

impl<'a, 'b, 'ctx> Compile<'a, 'b, 'ctx> {
    /// Compiles all the modules into object files.
    ///
    /// The return value is a list of file paths of the object files.
    pub(crate) fn run_all(
        state: &'a State,
        directories: &BuildDirectories,
        mir: &'a Mir,
    ) -> Result<Vec<PathBuf>, String> {
        let context = Context::new();
        let types = Layouts::new(state, mir, &context);
        let names = SymbolNames::new(&state.db, mir);
        let mut modules = Vec::with_capacity(mir.modules.len());

        for module_index in 0..mir.modules.len() {
            let mod_id = mir.modules[module_index].id;
            let name = mod_id.name(&state.db).clone();
            let path = mod_id.file(&state.db);
            let mut module = Module::new(&context, &types, name, &path);

            Compile {
                db: &state.db,
                mir,
                module_index,
                names: &names,
                context: &context,
                module: &mut module,
                layouts: &types,
                functions: HashMap::new(),
            }
            .run();

            modules.push(module);
        }

        let main_module = Module::new(
            &context,
            &types,
            ModuleName::new("$main"),
            Path::new("$main.inko"),
        );

        GenerateMain::new(
            &state.db,
            mir,
            &types,
            &names,
            &context,
            &main_module,
        )
        .run();

        modules.push(main_module);

        match state.config.target.arch {
            Architecture::Amd64 => {
                Target::initialize_x86(&InitializationConfig::default());
            }
            Architecture::Arm64 => {
                Target::initialize_aarch64(&InitializationConfig::default());
            }
        }

        // LLVM's optimisation level controls which passes to run, but some/many
        // of those may not be relevant to Inko, while slowing down compile
        // times. Thus instead of using this knob, we provide our own list of
        // passes. Swift and Rust (and possibly others) take a similar approach.
        let opt = OptimizationLevel::None;
        let reloc = RelocMode::PIC;
        let model = CodeModel::Default;
        let triple = TargetTriple::create(&state.config.target.llvm_triple());
        let target = Target::from_triple(&triple).unwrap();
        let target_machine = target
            .create_target_machine(&triple, "", "", opt, reloc, model)
            .unwrap();
        let layout = target_machine.get_target_data().get_data_layout();
        let pm_builder = PassManagerBuilder::create();
        let pm = PassManager::create(());

        pm_builder.set_optimization_level(opt);
        pm_builder.populate_module_pass_manager(&pm);
        pm.add_promote_memory_to_register_pass();

        for module in &modules {
            module.set_data_layout(&layout);
            module.set_triple(&triple);
            pm.run_on(&module.inner);
        }

        let mut paths = Vec::with_capacity(modules.len());

        for module in &modules {
            let path = directories
                .objects
                .join(format!("{}.o", module.name.normalized_name()));

            target_machine
                .write_to_file(&module.inner, FileType::Object, path.as_path())
                .map_err(|err| {
                    format!("Failed to create {}: {}", path.display(), err)
                })?;

            paths.push(path);
        }

        Ok(paths)
    }

    pub(crate) fn run(mut self) {
        for &class_id in &self.mir.modules[self.module_index].classes {
            for method_id in &self.mir.classes[&class_id].methods {
                let func = LowerMethod::new(
                    self.db,
                    self.mir,
                    self.layouts,
                    self.context,
                    self.names,
                    self.module,
                    &self.mir.methods[method_id],
                )
                .run();

                self.functions
                    .entry(class_id)
                    .or_insert_with(Vec::new)
                    .push(func);
            }
        }

        self.generate_setup_function();
        self.module.debug_builder.finalize();
    }

    fn generate_setup_function(&mut self) {
        let mod_id = self.mir.modules[self.module_index].id;
        let space = AddressSpace::default();
        let fn_name = &self.names.setup_functions[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.context, fn_val);
        let entry_block = self.context.append_basic_block(fn_val);

        builder.switch_to_block(entry_block);

        let state_var = builder.alloca(self.layouts.state.ptr_type(space));
        let method_var = builder.alloca(self.layouts.method);

        builder.store(state_var, fn_val.get_nth_param(0).unwrap());

        let body = self.context.append_basic_block(fn_val);

        builder.jump(body);
        builder.switch_to_block(body);

        // Allocate all classes defined in this module, and store them in their
        // corresponding globals.
        for &class_id in &self.mir.modules[self.module_index].classes {
            let raw_name = class_id.name(self.db);
            let name_ptr = builder.string_literal(raw_name).0.into();
            let fields_len = self
                .context
                .i8_type()
                .const_int(class_id.number_of_fields(self.db) as _, false)
                .into();
            let methods_len = self
                .context
                .i16_type()
                .const_int(
                    (self.layouts.methods(class_id) as usize) as _,
                    false,
                )
                .into();

            let class_new = if class_id.kind(self.db).is_async() {
                self.module.runtime_function(RuntimeFunction::ClassProcess)
            } else {
                self.module.runtime_function(RuntimeFunction::ClassObject)
            };

            let layout = self.layouts.classes[&class_id];
            let global_name = &self.names.classes[&class_id];
            let global = self.module.add_class(class_id, global_name);

            // The class globals must have an initializer, otherwise LLVM treats
            // them as external globals.
            global.set_initializer(
                &layout.ptr_type(space).const_null().as_basic_value_enum(),
            );

            let state = builder.load_pointer(self.layouts.state, state_var);

            // Built-in classes are defined in the runtime library, so we should
            // look them up instead of creating a new one.
            let class_ptr = if class_id.is_builtin() {
                // The first three fields in the State type are the singletons,
                // followed by the built-in classes, hence the offset of 3.
                builder
                    .load_field(self.layouts.state, state, class_id.0 + 3)
                    .into_pointer_value()
            } else {
                builder
                    .call(class_new, &[name_ptr, fields_len, methods_len])
                    .into_pointer_value()
            };

            for method in &self.mir.classes[&class_id].methods {
                let info = &self.layouts.methods[method];
                let name = &self.names.methods[method];
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

                builder.store_field(layout, method_var, hash_idx, hash);
                builder.store_field(layout, method_var, func_idx, func);

                let method = builder.load(layout, method_var);

                builder.store(method_addr, method);
            }

            builder.store(global.as_pointer_value(), class_ptr);
        }

        // Populate the globals for the constants defined in this module.
        for &cid in &self.mir.modules[self.module_index].constants {
            let name = &self.names.constants[&cid];
            let global = self.module.add_constant(name);
            let value = &self.mir.constants[&cid];

            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );
            self.set_constant_global(&builder, state_var, value, global);
        }

        // Populate the globals for the literals defined in this module.
        for (value, global) in &self.module.literals {
            self.set_constant_global(&builder, state_var, value, *global);
        }

        builder.return_value(None);
    }

    fn set_constant_global(
        &self,
        builder: &Builder<'ctx>,
        state_var: PointerValue<'ctx>,
        constant: &Constant,
        global: GlobalValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let global = global.as_pointer_value();
        let value = self.permanent_value(builder, state_var, constant);

        builder.store(global, value);
        global
    }

    fn permanent_value(
        &self,
        builder: &Builder<'ctx>,
        state_var: PointerValue<'ctx>,
        constant: &Constant,
    ) -> BasicValueEnum<'ctx> {
        let state = builder.load_pointer(self.layouts.state, state_var).into();

        match constant {
            Constant::Int(val) => {
                if let Some(ptr) = builder.tagged_int(*val) {
                    ptr.into()
                } else {
                    let val = builder.i64_literal(*val).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::IntBoxedPermanent);

                    builder.call(func, &[state, val])
                }
            }
            Constant::Float(val) => {
                let val = builder.context.f64_type().const_float(*val).into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::FloatBoxedPermanent);

                builder.call(func, &[state, val])
            }
            Constant::String(val) => {
                let bytes_typ =
                    builder.context.i8_type().array_type(val.len() as _);
                let bytes_var = builder.alloca(bytes_typ);
                let bytes = builder.string_bytes(val);

                builder.store(bytes_var, bytes);

                let len = builder.u64_literal(val.len() as u64).into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::StringNewPermanent);

                builder.call(func, &[state, bytes_var.into(), len])
            }
            Constant::Array(values) => {
                let len = builder.u64_literal(values.len() as u64).into();
                let new_func = self
                    .module
                    .runtime_function(RuntimeFunction::ArrayNewPermanent);
                let push_func =
                    self.module.runtime_function(RuntimeFunction::ArrayPush);
                let array = builder.call(new_func, &[state, len]);

                for val in values.iter() {
                    let ptr = self
                        .permanent_value(builder, state_var, val)
                        .into_pointer_value();

                    builder.call(push_func, &[state, array.into(), ptr.into()]);
                }

                array
            }
        }
    }
}

/// A pass for lowering the MIR of a single method.
pub struct LowerMethod<'a, 'b, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    layouts: &'a Layouts<'ctx>,

    /// The MIR method that we're lowering to LLVM.
    method: &'b Method,

    /// A map of method names to their mangled names.
    ///
    /// We cache these so we don't have to recalculate them on every reference.
    names: &'a SymbolNames,

    /// The builder to use for generating instructions.
    builder: Builder<'ctx>,

    /// The LLVM module the generated code belongs to.
    module: &'b mut Module<'a, 'ctx>,

    /// MIR registers and their corresponding LLVM stack variables.
    variables: HashMap<RegisterId, PointerValue<'ctx>>,

    /// The LLVM types for each MIR register.
    variable_types: HashMap<RegisterId, BasicTypeEnum<'ctx>>,
}

impl<'a, 'b, 'ctx> LowerMethod<'a, 'b, 'ctx> {
    fn new(
        db: &'a Database,
        mir: &'a Mir,
        layouts: &'a Layouts<'ctx>,
        context: &'ctx Context,
        names: &'a SymbolNames,
        module: &'b mut Module<'a, 'ctx>,
        method: &'b Method,
    ) -> Self {
        let function = module.add_method(&names.methods[&method.id], method.id);
        let builder = Builder::new(context, function);

        LowerMethod {
            db,
            mir,
            layouts,
            method,
            names,
            module,
            builder,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
        }
    }

    fn run(&mut self) -> FunctionValue<'ctx> {
        if self.method.id.is_async(self.db) {
            self.async_method();
        } else {
            self.regular_method();
        }

        self.builder.function
    }

    fn regular_method(&mut self) {
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let space = AddressSpace::default();
        let state_var =
            self.builder.new_stack_slot(self.layouts.state.ptr_type(space));
        let proc_var =
            self.builder.new_stack_slot(self.builder.context.pointer_type());

        // Build the stores for all the arguments, including the generated ones.
        self.builder.store(state_var, self.builder.argument(0));
        self.builder.store(proc_var, self.builder.argument(1));

        self.define_register_variables();

        for (arg, reg) in
            self.builder.arguments().skip(2).zip(self.method.arguments.iter())
        {
            self.builder.store(self.variables[reg], arg);
        }

        let (line, _) = self.mir.location(self.method.location).line_column();
        let debug_func = self.module.debug_builder.new_function(
            self.method.id.name(self.db),
            &self.names.methods[&self.method.id],
            line,
            self.method.id.is_private(self.db),
            false,
        );

        self.builder.set_debug_function(debug_func);
        self.method_body(state_var, proc_var);
    }

    fn async_method(&mut self) {
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let space = AddressSpace::default();
        let state_typ = self.layouts.state.ptr_type(space);
        let state_var = self.builder.new_stack_slot(state_typ);
        let proc_var =
            self.builder.new_stack_slot(self.builder.context.pointer_type());
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

        self.builder.store(
            state_var,
            self.builder.load_field(
                self.layouts.context,
                ctx,
                CONTEXT_STATE_INDEX,
            ),
        );
        self.builder.store(
            proc_var,
            self.builder.load_field(
                self.layouts.context,
                ctx,
                CONTEXT_PROCESS_INDEX,
            ),
        );

        let args = self
            .builder
            .load_field(self.layouts.context, ctx, CONTEXT_ARGS_INDEX)
            .into_pointer_value();

        self.builder.store(args_var, args);

        // For async methods we don't include the receiver in the message, as
        // this is redundant, and keeps message sizes as compact as possible.
        // Instead, we load the receiver from the context.
        let self_var = self.variables[&self.method.arguments[0]];

        self.builder.store(
            self_var,
            self.builder.load(self.builder.context.pointer_type(), proc_var),
        );

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

        let (line, _) = self.mir.location(self.method.location).line_column();
        let debug_func = self.module.debug_builder.new_function(
            self.method.id.name(self.db),
            &self.names.methods[&self.method.id],
            line,
            self.method.id.is_private(self.db),
            false,
        );

        self.builder.set_debug_function(debug_func);
        self.method_body(state_var, proc_var);
    }

    fn method_body(
        &mut self,
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
    ) {
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
                self.instruction(&llvm_blocks, state_var, proc_var, ins);
            }

            for &child in &mir_block.successors {
                if visited.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    fn instruction(
        &mut self,
        all_blocks: &[BasicBlock],
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
        ins: &Instruction,
    ) {
        match ins {
            Instruction::CallBuiltin(ins) => {
                self.set_debug_location(ins.location);

                match ins.name {
                    BuiltinFunction::IntAdd => {
                        self.checked_int_operation(
                            "llvm.sadd.with.overflow",
                            state_var,
                            proc_var,
                            self.variables[&ins.register],
                            self.variables[&ins.arguments[0]],
                            self.variables[&ins.arguments[1]],
                        );
                    }
                    BuiltinFunction::IntSub => {
                        self.checked_int_operation(
                            "llvm.ssub.with.overflow",
                            state_var,
                            proc_var,
                            self.variables[&ins.register],
                            self.variables[&ins.arguments[0]],
                            self.variables[&ins.arguments[1]],
                        );
                    }
                    BuiltinFunction::IntMul => {
                        self.checked_int_operation(
                            "llvm.smul.with.overflow",
                            state_var,
                            proc_var,
                            self.variables[&ins.register],
                            self.variables[&ins.arguments[0]],
                            self.variables[&ins.arguments[1]],
                        );
                    }
                    BuiltinFunction::IntDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);

                        self.check_division_overflow(proc_var, lhs, rhs);

                        let raw = self.builder.int_div(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRem => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);

                        self.check_division_overflow(proc_var, lhs, rhs);

                        let raw = self.builder.int_rem(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitAnd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.bit_and(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitOr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.bit_or(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitNot => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_int(val_var);
                        let raw = self.builder.bit_not(val);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntBitXor => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.bit_xor(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_eq(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_gt(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_ge(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_le(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_lt(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntPow => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let lhs = self.read_int(lhs_var).into();
                        let rhs = self.read_int(rhs_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::IntPow);
                        let raw = self
                            .builder
                            .call(func, &[proc, lhs, rhs])
                            .into_int_value();
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_add(lhs, rhs);
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_sub(lhs, rhs);
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatDiv => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_div(lhs, rhs);
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_mul(lhs, rhs);
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatMod => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_rem(
                            self.builder.float_add(
                                self.builder.float_rem(lhs, rhs),
                                rhs,
                            ),
                            rhs,
                        );
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatCeil => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.ceil",
                            &[self.builder.context.f64_type().into()],
                        );
                        let raw = self
                            .builder
                            .call(func, &[val.into()])
                            .into_float_value();
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatFloor => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var);
                        let func = self.module.intrinsic(
                            "llvm.floor",
                            &[self.builder.context.f64_type().into()],
                        );
                        let raw = self
                            .builder
                            .call(func, &[val.into()])
                            .into_float_value();
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatRound => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let lhs = self.read_float(lhs_var).into();
                        let rhs = self.read_int(rhs_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::FloatRound);
                        let res = self.builder.call(func, &[state, lhs, rhs]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let lhs = self.read_float(lhs_var).into();
                        let rhs = self.read_float(rhs_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::FloatEq);
                        let res = self.builder.call(func, &[state, lhs, rhs]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatToBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var);
                        let bits = self
                            .builder
                            .bitcast(val, self.builder.context.i64_type())
                            .into_int_value();
                        let res = self.new_int(state_var, bits);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatFromBits => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_int(val_var);
                        let bits = self
                            .builder
                            .bitcast(val, self.builder.context.f64_type())
                            .into_float_value();
                        let res = self.new_float(state_var, bits);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatGt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_gt(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatGe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_ge(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatLt => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_lt(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatLe => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_float(lhs_var);
                        let rhs = self.read_float(rhs_var);
                        let raw = self.builder.float_le(lhs, rhs);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatIsInf => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var);
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
                        let res = self.new_bool(state_var, cond);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatIsNan => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var);
                        let raw = self.builder.float_is_nan(val);
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatToInt => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_float(val_var).into();
                        let func = self.module.intrinsic(
                            "llvm.fptosi.sat",
                            &[
                                self.builder.context.i64_type().into(),
                                self.builder.context.f64_type().into(),
                            ],
                        );

                        let raw =
                            self.builder.call(func, &[val]).into_int_value();
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FloatToString => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.read_float(val_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::FloatToString);
                        let res = self.builder.call(func, &[state, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayCapacity => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let func_name = RuntimeFunction::ArrayCapacity;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayClear => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let func_name = RuntimeFunction::ArrayClear;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayDrop => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let func_name = RuntimeFunction::ArrayDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayGet => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let idx_var = self.variables[&ins.arguments[1]];
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let index = self.read_int(idx_var).into();
                        let func_name = RuntimeFunction::ArrayGet;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[array, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayLength => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let func_name = RuntimeFunction::ArrayLength;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayPop => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.builder.load_untyped_pointer(val_var);
                        let array = self.builder.untagged(val).into();
                        let func_name = RuntimeFunction::ArrayPop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayPush => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let value_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let tagged =
                            self.builder.load_untyped_pointer(array_var);
                        let array = self.builder.untagged(tagged).into();
                        let value =
                            self.builder.load_untyped_pointer(value_var).into();
                        let func_name = RuntimeFunction::ArrayPush;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, array, value]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayRemove => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let idx_var = self.variables[&ins.arguments[1]];
                        let val = self.builder.load_untyped_pointer(array_var);
                        let array = self.builder.untagged(val).into();
                        let idx = self.read_int(idx_var).into();
                        let func_name = RuntimeFunction::ArrayRemove;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[array, idx]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArrayReserve => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let amount_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.builder.load_untyped_pointer(array_var);
                        let array = self.builder.untagged(val).into();
                        let amount = self.read_int(amount_var).into();
                        let func_name = RuntimeFunction::ArrayReserve;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, array, amount]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ArraySet => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let value_var = self.variables[&ins.arguments[2]];
                        let tagged =
                            self.builder.load_untyped_pointer(array_var);
                        let array = self.builder.untagged(tagged).into();
                        let index = self.read_int(index_var).into();
                        let value =
                            self.builder.load_untyped_pointer(value_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ArraySet);
                        let res =
                            self.builder.call(func, &[array, index, value]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayNew => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayNew);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayAppend => {
                        let reg_var = self.variables[&ins.register];
                        let target_var = self.variables[&ins.arguments[0]];
                        let source_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let target = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(target_var),
                            )
                            .into();
                        let source = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(source_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayAppend);
                        let res =
                            self.builder.call(func, &[state, target, source]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayClear => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayClear);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayClone => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayClone);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayCopyFrom => {
                        let reg_var = self.variables[&ins.register];
                        let target_var = self.variables[&ins.arguments[0]];
                        let source_var = self.variables[&ins.arguments[1]];
                        let start_var = self.variables[&ins.arguments[2]];
                        let length_var = self.variables[&ins.arguments[3]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let target = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(target_var),
                            )
                            .into();
                        let source = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(source_var),
                            )
                            .into();
                        let start = self.read_int(start_var).into();
                        let length = self.read_int(length_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ByteArrayCopyFrom,
                        );
                        let res = self.builder.call(
                            func,
                            &[state, target, source, start, length],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayDrainToString => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ByteArrayDrainToString,
                        );
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayToString => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ByteArrayToString,
                        );
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayDrop => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayDrop);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let lhs = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(lhs_var),
                            )
                            .into();
                        let rhs = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(rhs_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayEq);
                        let res = self.builder.call(func, &[state, lhs, rhs]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayGet => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let index = self.read_int(index_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayGet);
                        let res = self.builder.call(func, &[array, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayLength => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayLength);
                        let res = self.builder.call(func, &[state, array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayPop => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayPop);
                        let res = self.builder.call(func, &[array]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayPush => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let value_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let value = self.read_int(value_var).into();
                        let func_name = RuntimeFunction::ByteArrayPush;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, array, value]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayRemove => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let index = self.read_int(index_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayRemove);
                        let res = self.builder.call(func, &[array, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArrayResize => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let size_var = self.variables[&ins.arguments[1]];
                        let fill_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let fill = self.read_int(fill_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArrayResize);
                        let res = self
                            .builder
                            .call(func, &[state, array, size, fill]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArraySet => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let value_var = self.variables[&ins.arguments[2]];
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let index = self.read_int(index_var).into();
                        let value = self.read_int(value_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArraySet);
                        let res =
                            self.builder.call(func, &[array, index, value]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ByteArraySlice => {
                        let reg_var = self.variables[&ins.register];
                        let array_var = self.variables[&ins.arguments[0]];
                        let start_var = self.variables[&ins.arguments[1]];
                        let length_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let array = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(array_var),
                            )
                            .into();
                        let start = self.read_int(start_var).into();
                        let length = self.read_int(length_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::ByteArraySlice);
                        let res = self
                            .builder
                            .call(func, &[state, array, start, length]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessSpawn => {
                        let reg_var = self.variables[&ins.register];
                        let program_var = self.variables[&ins.arguments[0]];
                        let args_var = self.variables[&ins.arguments[1]];
                        let env_var = self.variables[&ins.arguments[2]];
                        let stdin_var = self.variables[&ins.arguments[3]];
                        let stdout_var = self.variables[&ins.arguments[4]];
                        let stderr_var = self.variables[&ins.arguments[5]];
                        let dir_var = self.variables[&ins.arguments[6]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let program = self
                            .builder
                            .load_untyped_pointer(program_var)
                            .into();
                        let args =
                            self.builder.load_untyped_pointer(args_var).into();
                        let env =
                            self.builder.load_untyped_pointer(env_var).into();
                        let stdin = self.read_int(stdin_var).into();
                        let stdout = self.read_int(stdout_var).into();
                        let stderr = self.read_int(stderr_var).into();
                        let dir =
                            self.builder.load_untyped_pointer(dir_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessSpawn,
                        );
                        let res = self.builder.call(
                            func,
                            &[
                                proc, program, args, env, stdin, stdout,
                                stderr, dir,
                            ],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessDrop => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessDrop,
                        );
                        let res = self.builder.call(func, &[state, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStderrClose => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStderrClose,
                        );
                        let res = self.builder.call(func, &[state, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStderrRead => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let size_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStderrRead,
                        );
                        let res = self
                            .builder
                            .call(func, &[state, proc, child, buf, size]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdinClose => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdinClose,
                        );
                        let res = self.builder.call(func, &[state, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdinFlush => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdinFlush,
                        );
                        let res =
                            self.builder.call(func, &[state, proc, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdinWriteBytes => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdinWriteBytes,
                        );
                        let res =
                            self.builder.call(func, &[state, proc, child, buf]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdinWriteString => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let input_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let input =
                            self.builder.load_untyped_pointer(input_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdinWriteString,
                        );
                        let res = self
                            .builder
                            .call(func, &[state, proc, child, input]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdoutClose => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdoutClose,
                        );
                        let res = self.builder.call(func, &[state, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessStdoutRead => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let size_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessStdoutRead,
                        );
                        let res = self
                            .builder
                            .call(func, &[state, proc, child, buf, size]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessTryWait => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessTryWait,
                        );
                        let res = self.builder.call(func, &[child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChildProcessWait => {
                        let reg_var = self.variables[&ins.register];
                        let child_var = self.variables[&ins.arguments[0]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let child =
                            self.builder.load_untyped_pointer(child_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::ChildProcessWait,
                        );
                        let res = self.builder.call(func, &[proc, child]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::CpuCores => {
                        let reg_var = self.variables[&ins.register];
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::CpuCores);
                        let res = self.builder.call(func, &[]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::DirectoryCreate => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::DirectoryCreate;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::DirectoryCreateRecursive => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name =
                            RuntimeFunction::DirectoryCreateRecursive;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::DirectoryList => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::DirectoryList;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::DirectoryRemove => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::DirectoryRemove;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::DirectoryRemoveRecursive => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::DirectoryRemoveAll;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvArguments => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvArguments;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvExecutable => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvExecutable;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvGet => {
                        let reg_var = self.variables[&ins.register];
                        let name_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let name =
                            self.builder.load_untyped_pointer(name_var).into();
                        let func_name = RuntimeFunction::EnvGet;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, name]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvGetWorkingDirectory => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvGetWorkingDirectory;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvHomeDirectory => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvHomeDirectory;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvSetWorkingDirectory => {
                        let reg_var = self.variables[&ins.register];
                        let dir_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let dir =
                            self.builder.load_untyped_pointer(dir_var).into();
                        let func_name = RuntimeFunction::EnvSetWorkingDirectory;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, dir]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvTempDirectory => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvTempDirectory;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::EnvVariables => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::EnvVariables;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::Exit => {
                        let status_var = self.variables[&ins.arguments[0]];
                        let status = self.read_int(status_var).into();
                        let func_name = RuntimeFunction::Exit;
                        let func = self.module.runtime_function(func_name);

                        self.builder.call_void(func, &[status]);
                        self.builder.unreachable();
                    }
                    BuiltinFunction::FileCopy => {
                        let reg_var = self.variables[&ins.register];
                        let from_var = self.variables[&ins.arguments[0]];
                        let to_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let from =
                            self.builder.load_untyped_pointer(from_var).into();
                        let to =
                            self.builder.load_untyped_pointer(to_var).into();
                        let func_name = RuntimeFunction::FileCopy;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, from, to]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileDrop => {
                        let reg_var = self.variables[&ins.register];
                        let file_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let file =
                            self.builder.load_untyped_pointer(file_var).into();
                        let func_name = RuntimeFunction::FileDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, file]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileFlush => {
                        let reg_var = self.variables[&ins.register];
                        let file_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let file =
                            self.builder.load_untyped_pointer(file_var).into();
                        let func_name = RuntimeFunction::FileFlush;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, file]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileOpen => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let mode_var = self.variables[&ins.arguments[1]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let mode = self.read_int(mode_var).into();
                        let func_name = RuntimeFunction::FileOpen;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[proc, path, mode]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileRead => {
                        let reg_var = self.variables[&ins.register];
                        let file_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let size_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let file =
                            self.builder.load_untyped_pointer(file_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let func_name = RuntimeFunction::FileRead;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, file, buf, size]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileRemove => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::FileRemove;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileSeek => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let off_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let off = self.read_int(off_var).into();
                        let func_name = RuntimeFunction::FileSeek;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, path, off]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileSize => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::FileSize;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileWriteBytes => {
                        let reg_var = self.variables[&ins.register];
                        let file_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let file =
                            self.builder.load_untyped_pointer(file_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let func_name = RuntimeFunction::FileWriteBytes;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, file, buf]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::FileWriteString => {
                        let reg_var = self.variables[&ins.register];
                        let file_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let file =
                            self.builder.load_untyped_pointer(file_var).into();
                        let buf =
                            self.builder.load_untyped_pointer(buf_var).into();
                        let func_name = RuntimeFunction::FileWriteString;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, file, buf]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelReceive => {
                        let reg_var = self.variables[&ins.register];
                        let chan_var = self.variables[&ins.arguments[0]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let chan =
                            self.builder.load_untyped_pointer(chan_var).into();
                        let func_name = RuntimeFunction::ChannelReceive;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[proc, chan]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelReceiveUntil => {
                        let reg_var = self.variables[&ins.register];
                        let chan_var = self.variables[&ins.arguments[0]];
                        let time_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let chan =
                            self.builder.load_untyped_pointer(chan_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::ChannelReceiveUntil;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, chan, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelDrop => {
                        let reg_var = self.variables[&ins.register];
                        let chan_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let chan =
                            self.builder.load_untyped_pointer(chan_var).into();
                        let func_name = RuntimeFunction::ChannelDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, chan]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelWait => {
                        let reg_var = self.variables[&ins.register];
                        let chans_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var);
                        let proc = self.builder.load_untyped_pointer(proc_var);

                        // The standard library uses a reference in the wait()
                        // method, so we need to clear the reference bit before
                        // using the pointer.
                        let chans = self.builder.untagged(
                            self.builder.load_untyped_pointer(chans_var),
                        );
                        let func_name = RuntimeFunction::ChannelWait;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(
                            func,
                            &[state.into(), proc.into(), chans.into()],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelNew => {
                        let reg_var = self.variables[&ins.register];
                        let cap_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let cap = self.read_int(cap_var).into();
                        let func_name = RuntimeFunction::ChannelNew;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, cap]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelSend => {
                        let reg_var = self.variables[&ins.register];
                        let chan_var = self.variables[&ins.arguments[0]];
                        let msg_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let chan =
                            self.builder.load_untyped_pointer(chan_var).into();
                        let msg =
                            self.builder.load_untyped_pointer(msg_var).into();
                        let func_name = RuntimeFunction::ChannelSend;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, chan, msg]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ChannelTryReceive => {
                        let reg_var = self.variables[&ins.register];
                        let chan_var = self.variables[&ins.arguments[0]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let chan =
                            self.builder.load_untyped_pointer(chan_var).into();
                        let func_name = RuntimeFunction::ChannelTryReceive;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[proc, chan]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRotateLeft => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var).into();
                        let rhs = self.read_int(rhs_var).into();
                        let func = self.module.intrinsic(
                            "llvm.fshl",
                            &[self.builder.context.i64_type().into()],
                        );
                        let raw = self
                            .builder
                            .call(func, &[lhs, lhs, rhs])
                            .into_int_value();
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntRotateRight => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var).into();
                        let rhs = self.read_int(rhs_var).into();
                        let func = self.module.intrinsic(
                            "llvm.fshr",
                            &[self.builder.context.i64_type().into()],
                        );
                        let raw = self
                            .builder
                            .call(func, &[lhs, lhs, rhs])
                            .into_int_value();
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntShl => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);

                        self.check_shift_bits(proc_var, lhs, rhs);

                        let raw = self.builder.left_shift(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);

                        self.check_shift_bits(proc_var, lhs, rhs);

                        let raw = self.builder.signed_right_shift(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntUnsignedShr => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);

                        self.check_shift_bits(proc_var, lhs, rhs);

                        let raw = self.builder.right_shift(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntToFloat => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let val = self.read_int(val_var);
                        let raw = self.builder.int_to_float(val);
                        let res = self.new_float(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntToString => {
                        let reg_var = self.variables[&ins.register];
                        let val_var = self.variables[&ins.arguments[0]];
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::IntToString);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let val = self.read_int(val_var).into();
                        let ret = self.builder.call(func, &[state, val]);

                        self.builder.store(reg_var, ret);
                    }
                    BuiltinFunction::IntWrappingAdd => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_add(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntWrappingMul => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_mul(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::IntWrappingSub => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.read_int(lhs_var);
                        let rhs = self.read_int(rhs_var);
                        let raw = self.builder.int_sub(lhs, rhs);
                        let res = self.new_int(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ObjectEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let lhs = self.builder.load_untyped_pointer(lhs_var);
                        let rhs = self.builder.load_untyped_pointer(rhs_var);
                        let raw = self.builder.int_eq(
                            self.builder.pointer_to_int(lhs),
                            self.builder.pointer_to_int(rhs),
                        );
                        let res = self.new_bool(state_var, raw);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::Panic => {
                        let val_var = self.variables[&ins.arguments[0]];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::ProcessPanic;
                        let func = self.module.runtime_function(func_name);

                        self.builder.call_void(func, &[proc, val]);
                        self.builder.unreachable();
                    }
                    BuiltinFunction::PathAccessedAt => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathAccessedAt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathCreatedAt => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathCreatedAt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathModifiedAt => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathModifiedAt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathExpand => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathExpand;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathExists => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathExists;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathIsDirectory => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathIsDirectory;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::PathIsFile => {
                        let reg_var = self.variables[&ins.register];
                        let path_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let path =
                            self.builder.load_untyped_pointer(path_var).into();
                        let func_name = RuntimeFunction::PathIsFile;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, path]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStackFrameLine => {
                        let reg_var = self.variables[&ins.register];
                        let trace_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let func_name = RuntimeFunction::ProcessStackFrameLine;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let trace =
                            self.builder.load_untyped_pointer(trace_var).into();
                        let index = self.read_int(index_var).into();
                        let res =
                            self.builder.call(func, &[state, trace, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStackFrameName => {
                        let reg_var = self.variables[&ins.register];
                        let trace_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let func_name = RuntimeFunction::ProcessStackFrameName;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let trace =
                            self.builder.load_untyped_pointer(trace_var).into();
                        let index = self.read_int(index_var).into();
                        let res =
                            self.builder.call(func, &[state, trace, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStackFramePath => {
                        let reg_var = self.variables[&ins.register];
                        let trace_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let func_name = RuntimeFunction::ProcessStackFramePath;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let trace =
                            self.builder.load_untyped_pointer(trace_var).into();
                        let index = self.read_int(index_var).into();
                        let res =
                            self.builder.call(func, &[state, trace, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStacktrace => {
                        let reg_var = self.variables[&ins.register];
                        let func_name = RuntimeFunction::ProcessStacktrace;
                        let func = self.module.runtime_function(func_name);
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let res = self.builder.call(func, &[proc]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStacktraceDrop => {
                        let reg_var = self.variables[&ins.register];
                        let trace_var = self.variables[&ins.arguments[0]];
                        let func_name = RuntimeFunction::ProcessStacktraceDrop;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let trace =
                            self.builder.load_untyped_pointer(trace_var).into();
                        let res = self.builder.call(func, &[state, trace]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessStacktraceLength => {
                        let reg_var = self.variables[&ins.register];
                        let trace_var = self.variables[&ins.arguments[0]];
                        let func_name =
                            RuntimeFunction::ProcessStacktraceLength;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let trace =
                            self.builder.load_untyped_pointer(trace_var).into();
                        let res = self.builder.call(func, &[state, trace]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::ProcessSuspend => {
                        let reg_var = self.variables[&ins.register];
                        let time_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::ProcessSuspend;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomBytes => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let size_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let size = self.read_int(size_var).into();
                        let func_name = RuntimeFunction::RandomBytes;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, rng, size]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomDrop => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let func_name = RuntimeFunction::RandomDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, rng]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomFloat => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let func_name = RuntimeFunction::RandomFloat;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, rng]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomFloatRange => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let min_var = self.variables[&ins.arguments[1]];
                        let max_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let min = self.read_float(min_var).into();
                        let max = self.read_float(max_var).into();
                        let func_name = RuntimeFunction::RandomFloatRange;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, rng, min, max]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomFromInt => {
                        let reg_var = self.variables[&ins.register];
                        let seed_var = self.variables[&ins.arguments[0]];
                        let seed = self.read_int(seed_var).into();
                        let func_name = RuntimeFunction::RandomFromInt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[seed]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomInt => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let func_name = RuntimeFunction::RandomInt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, rng]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomIntRange => {
                        let reg_var = self.variables[&ins.register];
                        let rng_var = self.variables[&ins.arguments[0]];
                        let min_var = self.variables[&ins.arguments[1]];
                        let max_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let rng =
                            self.builder.load_untyped_pointer(rng_var).into();
                        let min = self.read_int(min_var).into();
                        let max = self.read_int(max_var).into();
                        let func_name = RuntimeFunction::RandomIntRange;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, rng, min, max]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::RandomNew => {
                        let reg_var = self.variables[&ins.register];
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let func_name = RuntimeFunction::RandomNew;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[proc]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketAccept => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let time_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketAccept;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, sock, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketAddressPairAddress => {
                        let reg_var = self.variables[&ins.register];
                        let pair_var = self.variables[&ins.arguments[0]];
                        let pair =
                            self.builder.load_untyped_pointer(pair_var).into();
                        let func_name =
                            RuntimeFunction::SocketAddressPairAddress;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[pair]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketAddressPairDrop => {
                        let reg_var = self.variables[&ins.register];
                        let pair_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let pair =
                            self.builder.load_untyped_pointer(pair_var).into();
                        let func_name = RuntimeFunction::SocketAddressPairDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, pair]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketAddressPairPort => {
                        let reg_var = self.variables[&ins.register];
                        let pair_var = self.variables[&ins.arguments[0]];
                        let pair =
                            self.builder.load_untyped_pointer(pair_var).into();
                        let func_name = RuntimeFunction::SocketAddressPairPort;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[pair]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketNew => {
                        let reg_var = self.variables[&ins.register];
                        let proto_var = self.variables[&ins.arguments[0]];
                        let kind_var = self.variables[&ins.arguments[1]];
                        let proto = self.read_int(proto_var).into();
                        let kind = self.read_int(kind_var).into();
                        let func_name = RuntimeFunction::SocketNew;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[proto, kind]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketBind => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let addr_var = self.variables[&ins.arguments[1]];
                        let port_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let addr =
                            self.builder.load_untyped_pointer(addr_var).into();
                        let port = self.read_int(port_var).into();
                        let func_name = RuntimeFunction::SocketBind;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, sock, addr, port]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketConnect => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let addr_var = self.variables[&ins.arguments[1]];
                        let port_var = self.variables[&ins.arguments[2]];
                        let time_var = self.variables[&ins.arguments[3]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let addr =
                            self.builder.load_untyped_pointer(addr_var).into();
                        let port = self.read_int(port_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketConnect;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, sock, addr, port, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketDrop => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketListen => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val = self.read_int(val_var).into();
                        let func_name = RuntimeFunction::SocketListen;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketLocalAddress => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketLocalAddress;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketPeerAddress => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketPeerAddress;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketRead => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let size_var = self.variables[&ins.arguments[2]];
                        let time_var = self.variables[&ins.arguments[3]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketRead;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, sock, buf, size, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketReceiveFrom => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let size_var = self.variables[&ins.arguments[2]];
                        let time_var = self.variables[&ins.arguments[3]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketReceiveFrom;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, sock, buf, size, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSendBytesTo => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let addr_var = self.variables[&ins.arguments[2]];
                        let port_var = self.variables[&ins.arguments[3]];
                        let time_var = self.variables[&ins.arguments[4]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let addr =
                            self.builder.load_untyped_pointer(addr_var).into();
                        let port = self.read_int(port_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketSendBytesTo;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(
                            func,
                            &[state, proc, sock, buf, addr, port, time],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSendStringTo => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let addr_var = self.variables[&ins.arguments[2]];
                        let port_var = self.variables[&ins.arguments[3]];
                        let time_var = self.variables[&ins.arguments[4]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf =
                            self.builder.load_untyped_pointer(buf_var).into();
                        let addr =
                            self.builder.load_untyped_pointer(addr_var).into();
                        let port = self.read_int(port_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketSendStringTo;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(
                            func,
                            &[state, proc, sock, buf, addr, port, time],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetBroadcast => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetBroadcast;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetKeepalive => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetKeepalive;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetLinger => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val = self.read_int(val_var).into();
                        let func_name = RuntimeFunction::SocketSetLinger;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetNodelay => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetNodelay;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetOnlyV6 => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetOnlyV6;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetRecvSize => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val = self.read_int(val_var).into();
                        let func_name = RuntimeFunction::SocketSetRecvSize;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetReuseAddress => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetReuseAddress;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetReusePort => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::SocketSetReusePort;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetSendSize => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val = self.read_int(val_var).into();
                        let func_name = RuntimeFunction::SocketSetSendSize;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketSetTtl => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let val_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let val = self.read_int(val_var).into();
                        let func_name = RuntimeFunction::SocketSetTtl;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock, val]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketShutdownRead => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketShutdownRead;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketShutdownReadWrite => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name =
                            RuntimeFunction::SocketShutdownReadWrite;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketShutdownWrite => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketShutdownWrite;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketTryClone => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let func_name = RuntimeFunction::SocketTryClone;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[sock]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketWriteBytes => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let time_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketWriteBytes;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, sock, buf, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::SocketWriteString => {
                        let reg_var = self.variables[&ins.register];
                        let sock_var = self.variables[&ins.arguments[0]];
                        let buf_var = self.variables[&ins.arguments[1]];
                        let time_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let sock =
                            self.builder.load_untyped_pointer(sock_var).into();
                        let buf =
                            self.builder.load_untyped_pointer(buf_var).into();
                        let time = self.read_int(time_var).into();
                        let func_name = RuntimeFunction::SocketWriteString;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, proc, sock, buf, time]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StderrFlush => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let func_name = RuntimeFunction::StderrFlush;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StderrWriteBytes => {
                        let reg_var = self.variables[&ins.register];
                        let input_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let input = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(input_var),
                            )
                            .into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::StderrWriteBytes,
                        );

                        let ret =
                            self.builder.call(func, &[state, proc, input]);

                        self.builder.store(reg_var, ret);
                    }
                    BuiltinFunction::StderrWriteString => {
                        let reg_var = self.variables[&ins.register];
                        let input_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let input =
                            self.builder.load_untyped_pointer(input_var).into();
                        let func = self.module.runtime_function(
                            RuntimeFunction::StderrWriteString,
                        );

                        let ret =
                            self.builder.call(func, &[state, proc, input]);

                        self.builder.store(reg_var, ret);
                    }
                    BuiltinFunction::StdinRead => {
                        let reg_var = self.variables[&ins.register];
                        let buf_var = self.variables[&ins.arguments[0]];
                        let size_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let size = self.read_int(size_var).into();
                        let func_name = RuntimeFunction::StdinRead;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, buf, size]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StdoutFlush => {
                        let reg_var = self.variables[&ins.register];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let func_name = RuntimeFunction::StdoutFlush;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StdoutWriteBytes => {
                        let reg_var = self.variables[&ins.register];
                        let buf_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let buf = self
                            .builder
                            .untagged(
                                self.builder.load_untyped_pointer(buf_var),
                            )
                            .into();
                        let func_name = RuntimeFunction::StdoutWriteBytes;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, proc, buf]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StdoutWriteString => {
                        let reg_var = self.variables[&ins.register];
                        let input_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let input =
                            self.builder.load_untyped_pointer(input_var).into();
                        let func_name = RuntimeFunction::StdoutWriteString;
                        let func = self.module.runtime_function(func_name);
                        let res =
                            self.builder.call(func, &[state, proc, input]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringByte => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let index_var = self.variables[&ins.arguments[1]];
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let index = self.read_int(index_var).into();
                        let func_name = RuntimeFunction::StringByte;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[string, index]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringCharacters => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringCharacters;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringCharactersDrop => {
                        let reg_var = self.variables[&ins.register];
                        let iter_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let iter =
                            self.builder.load_untyped_pointer(iter_var).into();
                        let func_name = RuntimeFunction::StringCharactersDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, iter]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringCharactersNext => {
                        let reg_var = self.variables[&ins.register];
                        let iter_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let iter =
                            self.builder.load_untyped_pointer(iter_var).into();
                        let func_name = RuntimeFunction::StringCharactersNext;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, iter]);

                        self.builder.store(reg_var, res);
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
                            let val = self
                                .builder
                                .load_untyped_pointer(self.variables[reg]);

                            self.builder.store_array_field(
                                temp_type, temp_var, idx as _, val,
                            );
                        }

                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let func_name = RuntimeFunction::StringConcat;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, temp_var.into(), len.into()]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringConcatArray => {
                        let reg_var = self.variables[&ins.register];
                        let ary_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let ary =
                            self.builder.load_untyped_pointer(ary_var).into();
                        let func_name = RuntimeFunction::StringConcatArray;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, ary]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringDrop => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringDrop;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringEq => {
                        let reg_var = self.variables[&ins.register];
                        let lhs_var = self.variables[&ins.arguments[0]];
                        let rhs_var = self.variables[&ins.arguments[1]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let lhs =
                            self.builder.load_untyped_pointer(lhs_var).into();
                        let rhs =
                            self.builder.load_untyped_pointer(rhs_var).into();
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::StringEquals);
                        let ret = self.builder.call(func, &[state, lhs, rhs]);

                        self.builder.store(reg_var, ret);
                    }
                    BuiltinFunction::StringSize => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringSize;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringSliceBytes => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let start_var = self.variables[&ins.arguments[1]];
                        let len_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let start = self.read_int(start_var).into();
                        let len = self.read_int(len_var).into();
                        let func_name = RuntimeFunction::StringSliceBytes;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, string, start, len]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringToByteArray => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringToByteArray;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringToFloat => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let start_var = self.variables[&ins.arguments[1]];
                        let end_var = self.variables[&ins.arguments[2]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let start = self.read_int(start_var).into();
                        let end = self.read_int(end_var).into();
                        let func_name = RuntimeFunction::StringToFloat;
                        let func = self.module.runtime_function(func_name);
                        let res = self
                            .builder
                            .call(func, &[state, string, start, end]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringToInt => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let radix_var = self.variables[&ins.arguments[1]];
                        let start_var = self.variables[&ins.arguments[2]];
                        let end_var = self.variables[&ins.arguments[3]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let radix = self.read_int(radix_var).into();
                        let start = self.read_int(start_var).into();
                        let end = self.read_int(end_var).into();
                        let func_name = RuntimeFunction::StringToInt;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(
                            func,
                            &[state, proc, string, radix, start, end],
                        );

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringToLower => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringToLower;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::StringToUpper => {
                        let reg_var = self.variables[&ins.register];
                        let string_var = self.variables[&ins.arguments[0]];
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let string = self
                            .builder
                            .load_untyped_pointer(string_var)
                            .into();
                        let func_name = RuntimeFunction::StringToUpper;
                        let func = self.module.runtime_function(func_name);
                        let res = self.builder.call(func, &[state, string]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::TimeMonotonic => {
                        let reg_var = self.variables[&ins.register];
                        let func_name = RuntimeFunction::TimeMonotonic;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::TimeSystem => {
                        let reg_var = self.variables[&ins.register];
                        let func_name = RuntimeFunction::TimeSystem;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::TimeSystemOffset => {
                        let reg_var = self.variables[&ins.register];
                        let func_name = RuntimeFunction::TimeSystemOffset;
                        let func = self.module.runtime_function(func_name);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var)
                            .into();
                        let res = self.builder.call(func, &[state]);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::HashKey0 => {
                        let reg_var = self.variables[&ins.register];
                        let typ = self.layouts.state;
                        let state = self.builder.load_pointer(typ, state_var);
                        let index = HASH_KEY0_INDEX;
                        let res = self.builder.load_field(typ, state, index);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::HashKey1 => {
                        let reg_var = self.variables[&ins.register];
                        let typ = self.layouts.state;
                        let state = self.builder.load_pointer(typ, state_var);
                        let index = HASH_KEY1_INDEX;
                        let res = self.builder.load_field(typ, state, index);

                        self.builder.store(reg_var, res);
                    }
                    BuiltinFunction::Moved => unreachable!(),
                }
            }
            Instruction::Goto(ins) => {
                self.builder.jump(all_blocks[ins.block.0]);
            }
            Instruction::Return(ins) => {
                let var = self.variables[&ins.register];
                let val =
                    self.builder.load(self.builder.context.pointer_type(), var);

                self.builder.return_value(Some(&val));
            }
            Instruction::Array(ins) => {
                let reg_var = self.variables[&ins.register];
                let state = self
                    .builder
                    .load_pointer(self.layouts.state, state_var)
                    .into();
                let len =
                    self.builder.u64_literal(ins.values.len() as u64).into();
                let new_func =
                    self.module.runtime_function(RuntimeFunction::ArrayNew);
                let push_func =
                    self.module.runtime_function(RuntimeFunction::ArrayPush);
                let array = self.builder.call(new_func, &[state, len]);

                for reg in ins.values.iter() {
                    let var = self.variables[reg];
                    let val = self
                        .builder
                        .load(self.builder.context.pointer_type(), var)
                        .into_pointer_value()
                        .into();

                    self.builder.call(push_func, &[state, array.into(), val]);
                }

                self.builder.store(reg_var, array);
            }
            Instruction::Branch(ins) => {
                let cond_var = self.variables[&ins.condition];
                let cond_ptr = self.builder.load_untyped_pointer(cond_var);

                // Load the `true` singleton from `State`.
                let state =
                    self.builder.load_pointer(self.layouts.state, state_var);
                let bool_ptr = self
                    .builder
                    .load_field(self.layouts.state, state, TRUE_INDEX)
                    .into_pointer_value();

                // Since our booleans are heap objects we have to
                // compare pointer addresses, and as such first have to
                // cast our pointers to ints.
                let cond_int = self.builder.pointer_to_int(cond_ptr);
                let bool_int = self.builder.pointer_to_int(bool_ptr);
                let cond = self.builder.int_eq(cond_int, bool_int);

                self.builder.branch(
                    cond,
                    all_blocks[ins.if_true.0],
                    all_blocks[ins.if_false.0],
                );
            }
            Instruction::Switch(ins) => {
                let reg_var = self.variables[&ins.register];
                let val = self.builder.load_untyped_pointer(reg_var);
                let addr = self.builder.pointer_to_int(val);
                let shift = self.builder.i64_literal(INT_SHIFT as i64);
                let untagged = self.builder.signed_right_shift(addr, shift);
                let mut cases = Vec::with_capacity(ins.blocks.len());

                for (index, block) in ins.blocks.iter().enumerate() {
                    cases.push((
                        self.builder.u64_literal(index as u64),
                        all_blocks[block.0],
                    ));
                }

                self.builder.exhaustive_switch(untagged, &cases);
            }
            Instruction::SwitchKind(ins) => {
                let val_var = self.variables[&ins.register];
                let kind_var = self.kind_of(val_var);
                let kind = self
                    .builder
                    .load(self.builder.context.i8_type(), kind_var)
                    .into_int_value();

                // Now we can generate the switch that jumps to the correct
                // block based on the value kind.
                let owned_block = all_blocks[ins.blocks[0].0];
                let ref_block = all_blocks[ins.blocks[1].0];
                let atomic_block = all_blocks[ins.blocks[2].0];
                let perm_block = all_blocks[ins.blocks[3].0];
                let int_block = all_blocks[ins.blocks[4].0];
                let float_block = all_blocks[ins.blocks[5].0];
                let cases = [
                    (self.builder.u8_literal(OWNED_KIND), owned_block),
                    (self.builder.u8_literal(REF_KIND), ref_block),
                    (self.builder.u8_literal(ATOMIC_KIND), atomic_block),
                    (self.builder.u8_literal(PERMANENT_KIND), perm_block),
                    (self.builder.u8_literal(INT_KIND), int_block),
                    (self.builder.u8_literal(FLOAT_KIND), float_block),
                ];

                self.builder.exhaustive_switch(kind, &cases);
            }
            Instruction::Nil(ins) => {
                let result = self.variables[&ins.register];
                let state =
                    self.builder.load_pointer(self.layouts.state, state_var);
                let val = self.builder.load_field(
                    self.layouts.state,
                    state,
                    NIL_INDEX,
                );

                self.builder.store(result, val);
            }
            Instruction::True(ins) => {
                let result = self.variables[&ins.register];
                let state =
                    self.builder.load_pointer(self.layouts.state, state_var);
                let val = self.builder.load_field(
                    self.layouts.state,
                    state,
                    TRUE_INDEX,
                );

                self.builder.store(result, val);
            }
            Instruction::False(ins) => {
                let result = self.variables[&ins.register];
                let state =
                    self.builder.load_pointer(self.layouts.state, state_var);
                let val = self.builder.load_field(
                    self.layouts.state,
                    state,
                    FALSE_INDEX,
                );

                self.builder.store(result, val);
            }
            Instruction::Int(ins) => {
                let var = self.variables[&ins.register];

                if let Some(ptr) = self.builder.tagged_int(ins.value) {
                    self.builder.store(var, ptr);
                } else {
                    let global = self
                        .module
                        .add_literal(&Constant::Int(ins.value))
                        .as_pointer_value();
                    let value = self.builder.load_untyped_pointer(global);

                    self.builder.store(var, value);
                }
            }
            Instruction::Float(ins) => {
                let var = self.variables[&ins.register];
                let global = self
                    .module
                    .add_literal(&Constant::Float(ins.value))
                    .as_pointer_value();
                let value = self.builder.load_untyped_pointer(global);

                self.builder.store(var, value);
            }
            Instruction::String(ins) => {
                let var = self.variables[&ins.register];
                let global = self
                    .module
                    .add_literal(&Constant::String(Rc::new(ins.value.clone())))
                    .as_pointer_value();
                let value = self.builder.load_untyped_pointer(global);

                self.builder.store(var, value);
            }
            Instruction::MoveRegister(ins) => {
                let source = self.variables[&ins.source];
                let target = self.variables[&ins.target];
                let typ = self.variable_types[&ins.source];

                self.builder.store(target, self.builder.load(typ, source));
            }
            Instruction::CallStatic(ins) => {
                self.set_debug_location(ins.location);

                let func_name = &self.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                ];

                for reg in &ins.arguments {
                    args.push(
                        self.builder
                            .load_untyped_pointer(self.variables[reg])
                            .into(),
                    );
                }

                self.call(ins.register, func, &args);
            }
            Instruction::CallInstance(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let func_name = &self.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    self.builder.load_untyped_pointer(rec_var).into(),
                ];

                for reg in &ins.arguments {
                    args.push(
                        self.builder
                            .load_untyped_pointer(self.variables[reg])
                            .into(),
                    );
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

                let index_type = self.builder.context.i64_type();
                let index_var = self.builder.new_stack_slot(index_type);
                let rec_var = self.variables[&ins.receiver];

                let rec = self.builder.load_untyped_pointer(rec_var);
                let info = &self.layouts.methods[&ins.method];
                let rec_class = self.class_of(rec);
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
                );

                let hash = self.builder.u64_literal(info.hash);

                self.builder.store(index_var, hash);

                let space = AddressSpace::default();
                let func_type = info.signature;
                let func_var =
                    self.builder.new_stack_slot(func_type.ptr_type(space));

                self.builder.jump(loop_start);

                // The start of the probing loop (probing is necessary).
                self.builder.switch_to_block(loop_start);

                // slot = index & len
                let index =
                    self.builder.load(index_type, index_var).into_int_value();
                let slot = self.builder.bit_and(index, len);
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
                        index_var,
                        self.builder
                            .int_add(index, self.builder.u64_literal(1)),
                    );
                    self.builder.jump(loop_start);
                } else {
                    self.builder.jump(after_loop);
                }

                // The block to jump to at the end of the loop, used for
                // calling the native function.
                self.builder.switch_to_block(after_loop);

                self.builder.store(
                    func_var,
                    self.builder.extract_field(method, METHOD_FUNCTION_INDEX),
                );

                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    rec.into(),
                ];

                for reg in &ins.arguments {
                    let val = self
                        .builder
                        .load_untyped_pointer(self.variables[reg])
                        .into();

                    args.push(val);
                }

                let func_val =
                    self.builder.load_function_pointer(func_type, func_var);

                self.indirect_call(ins.register, func_type, func_val, &args);
            }
            Instruction::CallClosure(ins) => {
                self.set_debug_location(ins.location);

                let rec_var = self.variables[&ins.receiver];
                let space = AddressSpace::default();

                // For closures we generate the signature on the fly, as the
                // method for `call` isn't always clearly defined: for an
                // argument typed as a closure, we don't know what the actual
                // method is, thus we can't retrieve an existing signature.
                let mut sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.layouts.state.ptr_type(space).into(), // State
                    self.builder.context.pointer_type().into(), // Process
                    self.builder.context.pointer_type().into(), // Closure
                ];

                for _ in &ins.arguments {
                    sig_args.push(self.builder.context.pointer_type().into());
                }

                // Load the method from the method table.
                let rec = self.builder.load_untyped_pointer(rec_var);
                let untagged = self.builder.untagged(rec);
                let class = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        untagged,
                        HEADER_CLASS_INDEX,
                    )
                    .into_pointer_value();

                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    rec.into(),
                ];

                for reg in &ins.arguments {
                    args.push(
                        self.builder
                            .load_untyped_pointer(self.variables[reg])
                            .into(),
                    );
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
                let space = AddressSpace::default();
                let sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.layouts.state.ptr_type(space).into(), // State
                    self.builder.context.pointer_type().into(), // Process
                    self.builder.context.pointer_type().into(), // Receiver
                ];

                let rec = self.builder.load_untyped_pointer(rec_var);
                let untagged = self.builder.untagged(rec);
                let class = self
                    .builder
                    .load_field(
                        self.layouts.header,
                        untagged,
                        HEADER_CLASS_INDEX,
                    )
                    .into_pointer_value();

                let state =
                    self.builder.load_pointer(self.layouts.state, state_var);
                let proc = self.builder.load_untyped_pointer(proc_var);
                let args: Vec<BasicMetadataValueEnum> =
                    vec![state.into(), proc.into(), rec.into()];

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
                let method_name = &self.names.methods[&ins.method];
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
                    let val =
                        self.builder.load_untyped_pointer(self.variables[reg]);
                    let slot = self.builder.u32_literal(index as u32);
                    let addr = self.builder.array_field_index_address(
                        self.layouts.message,
                        message,
                        MESSAGE_ARGUMENTS_INDEX,
                        slot,
                    );

                    self.builder.store(addr, val);
                }

                let state = self
                    .builder
                    .load_pointer(self.layouts.state, state_var)
                    .into();
                let sender = self.builder.load_untyped_pointer(proc_var).into();
                let receiver =
                    self.builder.load_untyped_pointer(rec_var).into();

                self.builder.call_void(
                    send_message,
                    &[state, sender, receiver, message.into()],
                );
            }
            Instruction::GetField(ins)
                if ins.class.kind(self.db).is_extern() =>
            {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let layout = self.layouts.instances[&ins.class];
                let index = ins.field.index(self.db) as u32;
                let rec =
                    self.builder.load(layout, rec_var).into_struct_value();
                let field = self.builder.extract_field(rec, index);

                self.builder.store(reg_var, field);
            }
            Instruction::SetField(ins)
                if ins.class.kind(self.db).is_extern() =>
            {
                let rec_var = self.variables[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let layout = self.layouts.instances[&ins.class];
                let index = ins.field.index(self.db) as u32;
                let val = self.builder.load_untyped_pointer(val_var);

                self.builder.store_field(layout, rec_var, index, val);
            }
            Instruction::GetField(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let base = if ins.class.kind(self.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index = (base + ins.field.index(self.db)) as u32;
                let layout = self.layouts.instances[&ins.class];
                let rec = self
                    .builder
                    .untagged(self.builder.load_untyped_pointer(rec_var));
                let field = self.builder.load_field(layout, rec, index);

                self.builder.store(reg_var, field);
            }
            Instruction::SetField(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let base = if ins.class.kind(self.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index = (base + ins.field.index(self.db)) as u32;
                let val = self.builder.load_untyped_pointer(val_var);
                let layout = self.layouts.instances[&ins.class];
                let rec = self
                    .builder
                    .untagged(self.builder.load_untyped_pointer(rec_var));

                self.builder.store_field(layout, rec, index, val);
            }
            Instruction::CheckRefs(ins) => {
                self.set_debug_location(ins.location);

                let var = self.variables[&ins.register];
                let proc = self.builder.load_untyped_pointer(proc_var).into();
                let check = self.builder.load_untyped_pointer(var).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::CheckRefs);

                self.builder.call_void(func, &[proc, check]);
            }
            Instruction::Free(ins) => {
                let var = self.variables[&ins.register];
                let free = self.builder.load_untyped_pointer(var).into();
                let func = self.module.runtime_function(RuntimeFunction::Free);

                self.builder.call_void(func, &[free]);
            }
            Instruction::Clone(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.source];
                let val = self.builder.load_untyped_pointer(val_var);

                match ins.kind {
                    CloneKind::Float => {
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var);
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::FloatClone);
                        let result = self
                            .builder
                            .call(func, &[state.into(), val.into()])
                            .into_pointer_value();

                        self.builder.store(reg_var, result);
                    }
                    CloneKind::Int => {
                        let addr = self.builder.pointer_to_int(val);
                        let mask = self.builder.i64_literal(INT_MASK);
                        let bits = self.builder.bit_and(addr, mask);
                        let cond = self.builder.int_eq(bits, mask);
                        let after_block = self.builder.add_block();
                        let tagged_block = self.builder.add_block();
                        let heap_block = self.builder.add_block();

                        self.builder.branch(cond, tagged_block, heap_block);

                        // The block to jump to when the Int is a tagged Int.
                        self.builder.switch_to_block(tagged_block);
                        self.builder.store(reg_var, val);
                        self.builder.jump(after_block);

                        // The block to jump to when the Int is a boxed Int.
                        self.builder.switch_to_block(heap_block);

                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::IntClone);
                        let state = self
                            .builder
                            .load_pointer(self.layouts.state, state_var);
                        let result = self
                            .builder
                            .call(func, &[state.into(), val.into()])
                            .into_pointer_value();

                        self.builder.store(reg_var, result);
                        self.builder.jump(after_block);

                        self.builder.switch_to_block(after_block);
                    }
                }
            }
            Instruction::Increment(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.value];
                let val = self.builder.load_untyped_pointer(val_var);
                let header = self.builder.untagged(val);
                let one = self.builder.u32_literal(1);
                let old = self
                    .builder
                    .load_field(self.layouts.header, header, HEADER_REFS_INDEX)
                    .into_int_value();
                let new = self.builder.int_add(old, one);

                self.builder.store_field(
                    self.layouts.header,
                    header,
                    HEADER_REFS_INDEX,
                    new,
                );

                let old_addr = self.builder.pointer_to_int(val);
                let mask = self.builder.i64_literal(REF_MASK);
                let new_addr = self.builder.bit_or(old_addr, mask);
                let ref_ptr = self.builder.int_to_pointer(new_addr);

                self.builder.store(reg_var, ref_ptr);
            }
            Instruction::Decrement(ins) => {
                let var = self.variables[&ins.register];
                let header = self
                    .builder
                    .untagged(self.builder.load_untyped_pointer(var));

                let old_refs = self
                    .builder
                    .load_field(self.layouts.header, header, HEADER_REFS_INDEX)
                    .into_int_value();
                let one = self.builder.u32_literal(1);
                let new_refs = self.builder.int_sub(old_refs, one);

                self.builder.store_field(
                    self.layouts.header,
                    header,
                    HEADER_REFS_INDEX,
                    new_refs,
                );
            }
            Instruction::IncrementAtomic(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.value];
                let val = self.builder.load_untyped_pointer(val_var);
                let one = self.builder.u32_literal(1);
                let field = self.builder.field_address(
                    self.layouts.header,
                    val,
                    HEADER_REFS_INDEX,
                );

                self.builder.atomic_add(field, one);
                self.builder.store(reg_var, val);
            }
            Instruction::DecrementAtomic(ins) => {
                let var = self.variables[&ins.register];
                let header =
                    self.builder.load_pointer(self.layouts.header, var);
                let decr_block = self.builder.add_block();
                let drop_block = all_blocks[ins.if_true.0];
                let after_block = all_blocks[ins.if_false.0];
                let kind = self
                    .builder
                    .load_field(self.layouts.header, header, HEADER_KIND_INDEX)
                    .into_int_value();
                let perm_kind = self.builder.u8_literal(PERMANENT_KIND);
                let is_perm = self.builder.int_eq(kind, perm_kind);

                self.builder.branch(is_perm, after_block, decr_block);

                // The block to jump to when the value isn't a permanent value,
                // and its reference count should be decremented.
                self.builder.switch_to_block(decr_block);

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
                if ins.class.kind(self.db).is_extern() =>
            {
                // Defining the alloca already reserves (uninitialised) memory,
                // so there's nothing we actually need to do here. Setting the
                // fields is done using separate instructions.
            }
            Instruction::Allocate(ins) => {
                let reg_var = self.variables[&ins.register];
                let name = &self.names.classes[&ins.class];
                let global =
                    self.module.add_class(ins.class, name).as_pointer_value();
                let class = self.builder.load_untyped_pointer(global);
                let func =
                    self.module.runtime_function(RuntimeFunction::Allocate);
                let ptr = self.builder.call(func, &[class.into()]);

                self.builder.store(reg_var, ptr);
            }
            Instruction::Spawn(ins) => {
                let reg_var = self.variables[&ins.register];
                let name = &self.names.classes[&ins.class];
                let global =
                    self.module.add_class(ins.class, name).as_pointer_value();
                let class = self.builder.load_untyped_pointer(global).into();
                let proc = self.builder.load_untyped_pointer(proc_var).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::ProcessNew);
                let ptr = self.builder.call(func, &[proc, class]);

                self.builder.store(reg_var, ptr);
            }
            Instruction::GetConstant(ins) => {
                let var = self.variables[&ins.register];
                let name = &self.names.constants[&ins.id];
                let global = self.module.add_constant(name).as_pointer_value();
                let value = self.builder.load_untyped_pointer(global);

                self.builder.store(var, value);
            }
            Instruction::Reduce(ins) => {
                let amount = self
                    .builder
                    .context
                    .i16_type()
                    .const_int(ins.amount as u64, false)
                    .into();
                let proc = self.builder.load_untyped_pointer(proc_var).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::Reduce);

                self.builder.call_void(func, &[proc, amount]);
            }
            Instruction::Finish(ins) => {
                let proc = self.builder.load_untyped_pointer(proc_var).into();
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
            Instruction::Reference(_) => unreachable!(),
            Instruction::Drop(_) => unreachable!(),
        }
    }

    fn kind_of(
        &mut self,
        pointer_variable: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        // Instead of fiddling with phi nodes we just inject a new stack slot in
        // the entry block and use that. clang takes a similar approach when
        // building switch() statements.
        let result =
            self.builder.new_stack_slot(self.builder.context.i8_type());
        let int_block = self.builder.add_block();
        let ref_block = self.builder.add_block();
        let header_block = self.builder.add_block();
        let after_block = self.builder.add_block();
        let pointer = self.builder.load_untyped_pointer(pointer_variable);
        let addr = self.builder.pointer_to_int(pointer);
        let mask = self.builder.i64_literal(TAG_MASK);
        let bits = self.builder.bit_and(addr, mask);

        // This generates the equivalent of the following:
        //
        //     match ptr as usize & MASK {
        //       INT_MASK => ...
        //       MASK     => ...
        //       REF_MASK => ...
        //       _        => ...
        //     }
        self.builder.switch(
            bits,
            &[
                (self.builder.i64_literal(INT_MASK), int_block),
                // Uneven tagged integers will have both the first and second
                // bit set to 1, so we also need to handle such values here.
                (self.builder.i64_literal(TAG_MASK), int_block),
                (self.builder.i64_literal(REF_MASK), ref_block),
            ],
            header_block,
        );

        // The case for when the value is a tagged integer.
        self.builder.switch_to_block(int_block);
        self.builder.store(result, self.builder.u8_literal(INT_KIND));
        self.builder.jump(after_block);

        // The case for when the value is a reference.
        self.builder.switch_to_block(ref_block);
        self.builder.store(result, self.builder.u8_literal(REF_KIND));
        self.builder.jump(after_block);

        // The fallback case where we read the kind from the object header. This
        // generates the equivalent of `(*(ptr as *mut Header)).kind`.
        self.builder.switch_to_block(header_block);

        let header_val = self
            .builder
            .load_field(self.layouts.header, pointer, HEADER_KIND_INDEX)
            .into_int_value();

        self.builder.store(result, header_val);
        self.builder.jump(after_block);
        self.builder.switch_to_block(after_block);
        result
    }

    fn class_of(&mut self, receiver: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let tagged_block = self.builder.add_block();
        let heap_block = self.builder.add_block();
        let after_block = self.builder.add_block();
        let class_var =
            self.builder.new_stack_slot(self.builder.context.pointer_type());
        let int_global = self
            .module
            .add_class(ClassId::int(), &self.names.classes[&ClassId::int()]);

        let addr = self.builder.pointer_to_int(receiver);
        let mask = self.builder.i64_literal(INT_MASK);
        let bits = self.builder.bit_and(addr, mask);
        let is_tagged = self.builder.int_eq(bits, mask);

        self.builder.branch(is_tagged, tagged_block, heap_block);

        // The block to jump to when the receiver is a tagged integer.
        self.builder.switch_to_block(tagged_block);
        self.builder.store(
            class_var,
            self.builder.load_untyped_pointer(int_global.as_pointer_value()),
        );
        self.builder.jump(after_block);

        // The block to jump to when the receiver is a heap object. In this case
        // we read the class from the (untagged) header.
        self.builder.switch_to_block(heap_block);

        let header = self.builder.untagged(receiver);
        let class = self
            .builder
            .load_field(self.layouts.header, header, HEADER_CLASS_INDEX)
            .into_pointer_value();

        self.builder.store(class_var, class);
        self.builder.jump(after_block);

        // The block to jump to to load the method pointer.
        self.builder.switch_to_block(after_block);
        self.builder.load_pointer(self.layouts.empty_class, class_var)
    }

    fn read_int(&mut self, variable: PointerValue<'ctx>) -> IntValue<'ctx> {
        let pointer = self.builder.load_untyped_pointer(variable);
        let res_type = self.builder.context.i64_type();
        let res_var = self.builder.new_stack_slot(res_type);
        let tagged_block = self.builder.add_block();
        let heap_block = self.builder.add_block();
        let after_block = self.builder.add_block();

        let addr = self.builder.pointer_to_int(pointer);
        let mask = self.builder.i64_literal(INT_MASK);
        let bits = self.builder.bit_and(addr, mask);
        let cond = self.builder.int_eq(bits, mask);

        self.builder.branch(cond, tagged_block, heap_block);

        // The block to jump to when the Int is a tagged Int.
        self.builder.switch_to_block(tagged_block);

        let shift = self.builder.i64_literal(INT_SHIFT as i64);
        let untagged = self.builder.signed_right_shift(addr, shift);

        self.builder.store(res_var, untagged);
        self.builder.jump(after_block);

        // The block to jump to when the Int is a heap Int.
        self.builder.switch_to_block(heap_block);

        let layout = self.layouts.instances[&ClassId::int()];

        self.builder.store(
            res_var,
            self.builder.load_field(layout, pointer, BOXED_INT_VALUE_INDEX),
        );
        self.builder.jump(after_block);

        self.builder.switch_to_block(after_block);
        self.builder.load(res_type, res_var).into_int_value()
    }

    fn read_float(&mut self, variable: PointerValue<'ctx>) -> FloatValue<'ctx> {
        let layout = self.layouts.instances[&ClassId::float()];
        let ptr = self.builder.load_pointer(layout, variable);

        self.builder
            .load_field(layout, ptr, BOXED_FLOAT_VALUE_INDEX)
            .into_float_value()
    }

    fn new_float(
        &mut self,
        state_var: PointerValue<'ctx>,
        value: FloatValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let func = self.module.runtime_function(RuntimeFunction::FloatBoxed);
        let state = self.builder.load_pointer(self.layouts.state, state_var);

        self.builder
            .call(func, &[state.into(), value.into()])
            .into_pointer_value()
    }

    fn checked_int_operation(
        &mut self,
        name: &str,
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
        reg_var: PointerValue<'ctx>,
        lhs_var: PointerValue<'ctx>,
        rhs_var: PointerValue<'ctx>,
    ) {
        let ok_block = self.builder.add_block();
        let err_block = self.builder.add_block();
        let after_block = self.builder.add_block();
        let lhs = self.read_int(lhs_var);
        let rhs = self.read_int(rhs_var);
        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let add = self
            .module
            .intrinsic(name, &[self.builder.context.i64_type().into()]);

        let res = self
            .builder
            .call(add, &[lhs.into(), rhs.into()])
            .into_struct_value();

        // Check if we overflowed the operation.
        let new_val = self
            .builder
            .extract_field(res, LLVM_RESULT_VALUE_INDEX)
            .into_int_value();
        let overflow = self
            .builder
            .extract_field(res, LLVM_RESULT_STATUS_INDEX)
            .into_int_value();

        self.builder.branch(overflow, err_block, ok_block);

        // The block to jump to if the operation didn't overflow.
        {
            self.builder.switch_to_block(ok_block);

            let val = self.new_int(state_var, new_val);

            self.builder.store(reg_var, val);
            self.builder.jump(after_block);
        }

        // The block to jump to if the operation overflowed.
        self.builder.switch_to_block(err_block);

        let proc = self.builder.load_untyped_pointer(proc_var);

        self.builder.call_void(func, &[proc.into(), lhs.into(), rhs.into()]);
        self.builder.unreachable();
        self.builder.switch_to_block(after_block);
    }

    fn new_int(
        &mut self,
        state_var: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let res_var =
            self.builder.new_stack_slot(self.builder.context.pointer_type());
        let tagged_block = self.builder.add_block();
        let heap_block = self.builder.add_block();
        let after_block = self.builder.add_block();
        let and_block = self.builder.add_block();

        let min = self.builder.i64_literal(MIN_INT);
        let max = self.builder.i64_literal(MAX_INT);

        self.builder.branch(
            self.builder.int_ge(value, min),
            and_block,
            heap_block,
        );

        // The block to jump to when we're larger than or equal to the minimum
        // value for a tagged Int.
        self.builder.switch_to_block(and_block);
        self.builder.branch(
            self.builder.int_le(value, max),
            tagged_block,
            heap_block,
        );

        // The block to jump to when the Int fits in a tagged pointer.
        self.builder.switch_to_block(tagged_block);

        let shift = self.builder.i64_literal(INT_SHIFT as i64);
        let mask = self.builder.i64_literal(INT_MASK);
        let addr =
            self.builder.bit_or(self.builder.left_shift(value, shift), mask);

        self.builder.store(res_var, self.builder.int_to_pointer(addr));
        self.builder.jump(after_block);

        // The block to jump to when the Int must be boxed.
        self.builder.switch_to_block(heap_block);

        let func = self.module.runtime_function(RuntimeFunction::IntBoxed);
        let state = self.builder.load_pointer(self.layouts.state, state_var);
        let res = self.builder.call(func, &[state.into(), value.into()]);

        self.builder.store(res_var, res);
        self.builder.jump(after_block);

        self.builder.switch_to_block(after_block);
        self.builder.load_untyped_pointer(res_var)
    }

    fn new_bool(
        &mut self,
        state_var: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let result =
            self.builder.new_stack_slot(self.builder.context.pointer_type());
        let state = self.builder.load_pointer(self.layouts.state, state_var);
        let true_block = self.builder.add_block();
        let false_block = self.builder.add_block();
        let after_block = self.builder.add_block();

        self.builder.branch(value, true_block, false_block);

        // The block to jump to when the condition is true.
        self.builder.switch_to_block(true_block);
        self.builder.store(
            result,
            self.builder.load_field(self.layouts.state, state, TRUE_INDEX),
        );
        self.builder.jump(after_block);

        // The block to jump to when the condition is false.
        self.builder.switch_to_block(false_block);
        self.builder.store(
            result,
            self.builder.load_field(self.layouts.state, state, FALSE_INDEX),
        );
        self.builder.jump(after_block);

        self.builder.switch_to_block(after_block);
        self.builder.load_untyped_pointer(result)
    }

    fn check_division_overflow(
        &self,
        process_var: PointerValue<'ctx>,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) {
        let min = self.builder.i64_literal(i64::MIN);
        let minus_one = self.builder.i64_literal(-1);
        let zero = self.builder.i64_literal(0);
        let and_block = self.builder.add_block();
        let or_block = self.builder.add_block();
        let overflow_block = self.builder.add_block();
        let ok_block = self.builder.add_block();

        // lhs == MIN AND rhs == -1
        self.builder.branch(self.builder.int_eq(lhs, min), and_block, or_block);

        self.builder.switch_to_block(and_block);
        self.builder.branch(
            self.builder.int_eq(rhs, minus_one),
            overflow_block,
            or_block,
        );

        // OR rhs == 0
        self.builder.switch_to_block(or_block);
        self.builder.branch(
            self.builder.int_eq(rhs, zero),
            overflow_block,
            ok_block,
        );

        // The block to jump to if an overflow would occur.
        self.builder.switch_to_block(overflow_block);

        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let proc = self.builder.load_untyped_pointer(process_var);

        self.builder.call_void(func, &[proc.into(), lhs.into(), rhs.into()]);
        self.builder.unreachable();

        // The block to jump to when it's safe to perform the
        // operation.
        self.builder.switch_to_block(ok_block);
    }

    fn check_shift_bits(
        &self,
        process_var: PointerValue<'ctx>,
        value: IntValue<'ctx>,
        bits: IntValue<'ctx>,
    ) {
        let ok_block = self.builder.add_block();
        let err_block = self.builder.add_block();
        let min = self.builder.i64_literal((i64::BITS - 1) as _);
        let cond = self.builder.int_gt(bits, min);

        self.builder.branch(cond, err_block, ok_block);

        // The block to jump to when the operation would overflow.
        self.builder.switch_to_block(err_block);

        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let proc = self.builder.load_untyped_pointer(process_var);

        self.builder.call_void(func, &[proc.into(), value.into(), bits.into()]);
        self.builder.unreachable();

        // The block to jump to when all is well.
        self.builder.switch_to_block(ok_block);
    }

    fn define_register_variables(&mut self) {
        let space = AddressSpace::default();

        for index in 0..self.method.registers.len() {
            let id = RegisterId(index as _);
            let typ = self.method.registers.value_type(id);
            let alloca_typ = if let Some(id) = typ.class_id(self.db) {
                let layout = self.layouts.instances[&id];

                if id.kind(self.db).is_extern() {
                    layout.as_basic_type_enum()
                } else {
                    layout.ptr_type(space).as_basic_type_enum()
                }
            } else {
                self.builder.context.pointer_type().as_basic_type_enum()
            };

            self.variables.insert(id, self.builder.alloca(alloca_typ));
            self.variable_types.insert(id, alloca_typ);
        }
    }

    fn register_type(&self, register: RegisterId) -> types::TypeRef {
        self.method.registers.value_type(register)
    }

    fn call(
        &self,
        register: RegisterId,
        function: FunctionValue<'ctx>,
        arguments: &[BasicMetadataValueEnum],
    ) {
        let var = self.variables[&register];

        if self.register_type(register).is_never(self.db) {
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

        if self.register_type(register).is_never(self.db) {
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
        let (line, col) = self.mir.location(location_id).line_column();
        let loc = self.module.debug_builder.new_location(line, col, scope);

        self.builder.set_debug_location(loc);
    }
}

/// A pass for generating the entry module and method (i.e. `main()`).
pub(crate) struct GenerateMain<'a, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    layouts: &'a Layouts<'ctx>,
    names: &'a SymbolNames,
    context: &'ctx Context,
    module: &'a Module<'a, 'ctx>,
    builder: Builder<'ctx>,
}

impl<'a, 'ctx> GenerateMain<'a, 'ctx> {
    fn new(
        db: &'a Database,
        mir: &'a Mir,
        layouts: &'a Layouts<'ctx>,
        names: &'a SymbolNames,
        context: &'ctx Context,
        module: &'a Module<'a, 'ctx>,
    ) -> GenerateMain<'a, 'ctx> {
        let typ = context.i32_type().fn_type(&[], false);
        let function = module.add_function("main", typ, None);
        let builder = Builder::new(context, function);

        GenerateMain { db, mir, layouts, names, context, module, builder }
    }

    fn run(self) {
        let space = AddressSpace::default();
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let layout = self.layouts.method_counts;
        let counts = self.builder.alloca(layout);

        self.set_method_count(counts, ClassId::int());
        self.set_method_count(counts, ClassId::float());
        self.set_method_count(counts, ClassId::string());
        self.set_method_count(counts, ClassId::array());
        self.set_method_count(counts, ClassId::boolean());
        self.set_method_count(counts, ClassId::nil());
        self.set_method_count(counts, ClassId::byte_array());
        self.set_method_count(counts, ClassId::channel());

        let rt_new = self.module.runtime_function(RuntimeFunction::RuntimeNew);
        let rt_start =
            self.module.runtime_function(RuntimeFunction::RuntimeStart);
        let rt_state =
            self.module.runtime_function(RuntimeFunction::RuntimeState);
        let rt_drop =
            self.module.runtime_function(RuntimeFunction::RuntimeDrop);
        let exit = self.module.runtime_function(RuntimeFunction::Exit);

        let runtime =
            self.builder.call(rt_new, &[counts.into()]).into_pointer_value();
        let state =
            self.builder.call(rt_state, &[runtime.into()]).into_pointer_value();

        // Call all the module setup functions. This is used to populate
        // constants, define classes, etc.
        for &id in self.mir.modules.keys() {
            let name = &self.names.setup_functions[&id];
            let func = self.module.add_setup_function(name);

            self.builder.call_void(func, &[state.into()]);
        }

        let main_class_id = self.db.main_class().unwrap();
        let main_method_id = self.db.main_method().unwrap();
        let main_class_ptr = self
            .module
            .add_global(&self.names.classes[&main_class_id])
            .as_pointer_value();

        let main_method = self
            .module
            .add_function(
                &self.names.methods[&main_method_id],
                self.context.void_type().fn_type(
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
        self.builder.call_void(exit, &[self.builder.i64_literal(0).into()]);
        self.builder.unreachable();
    }

    fn methods(&self, id: ClassId) -> IntValue<'ctx> {
        self.context.i16_type().const_int(self.layouts.methods(id) as _, false)
    }

    fn set_method_count(&self, counts: PointerValue<'ctx>, class: ClassId) {
        let layout = self.layouts.method_counts;

        self.builder.store_field(layout, counts, class.0, self.methods(class));
    }
}
