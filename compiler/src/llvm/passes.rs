use crate::config::{BuildDirectories, Opt};
use crate::llvm::builder::Builder;
use crate::llvm::constants::{
    ARRAY_BUF_INDEX, ARRAY_CAPA_INDEX, ARRAY_LENGTH_INDEX,
    CLASS_METHODS_COUNT_INDEX, CLASS_METHODS_INDEX, CLOSURE_CALL_INDEX,
    CONTEXT_ARGS_INDEX, CONTEXT_PROCESS_INDEX, CONTEXT_STATE_INDEX,
    DROPPER_INDEX, FIELD_OFFSET, HEADER_CLASS_INDEX, HEADER_REFS_INDEX,
    MESSAGE_ARGUMENTS_INDEX, METHOD_FUNCTION_INDEX, METHOD_HASH_INDEX,
    PROCESS_EPOCH_OFFSET, PROCESS_FIELD_OFFSET, STATE_EPOCH_OFFSET,
};
use crate::llvm::context::Context;
use crate::llvm::layouts::Layouts;
use crate::llvm::module::Module;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::mir::{
    CastType, Constant, Instruction, LocationId, Method, Mir, RegisterId,
};
use crate::state::State;
use crate::symbol_names::SymbolNames;
use crate::target::Architecture;
use inkwell::basic_block::BasicBlock;
use inkwell::module::Linkage;
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
use types::module_name::ModuleName;
use types::{
    BuiltinFunction, ClassId, Database, Shape, TypeRef, BYTE_ARRAY_ID,
    STRING_ID,
};

/// A compiler pass that compiles Inko MIR into object files using LLVM.
pub(crate) struct Compile<'a, 'b, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    module_index: usize,
    layouts: &'a Layouts<'ctx>,
    names: &'a SymbolNames,
    context: &'ctx Context,
    module: &'b mut Module<'a, 'ctx>,
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
        //
        // For the aggressive mode we simply enable the full suite of LLVM
        // optimizations, likely greatly increasing the compilation times.
        let opt = match state.config.opt {
            Opt::None => OptimizationLevel::None,

            // We have yet to figure out what optimizations we want to enable
            // here, hence we don't apply any at all.
            Opt::Balanced => OptimizationLevel::None,

            // This is the equivalent of -O3 for clang.
            Opt::Aggressive => OptimizationLevel::Aggressive,
        };

        let reloc = RelocMode::PIC;
        let model = CodeModel::Default;
        let triple = TargetTriple::create(&state.config.target.llvm_triple());
        let target = Target::from_triple(&triple).unwrap();
        let target_machine = target
            .create_target_machine(&triple, "", "", opt, reloc, model)
            .unwrap();

        let context = Context::new();
        let types = Layouts::new(
            state,
            mir,
            &context,
            target_machine.get_target_data(),
        );

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

        if state.config.write_llvm {
            directories.create_llvm()?;
        }

        if state.config.write_llvm {
            for module in &modules {
                let name = module.name.normalized_name();
                let path = directories.llvm_ir.join(format!("{}.ll", name));

                module.print_to_file(&path).map_err(|err| {
                    format!("Failed to create {}: {}", path.display(), err)
                })?;
            }
        }

        // We verify _after_ writing the LLVM IR (if enabled) such that the IR
        // can be inspected in the event of a verification failure.
        if state.config.verify_llvm {
            for module in &modules {
                if let Err(err) = module.verify() {
                    panic!(
                        "the LLVM module '{}' must be valid:\n\n{}\n",
                        module.name,
                        err.to_string(),
                    );
                }
            }
        }

        for module in &modules {
            let name = module.name.normalized_name();
            let path = directories.objects.join(format!("{}.o", name));

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
        for method in &self.mir.modules[self.module_index].methods {
            LowerMethod::new(
                self.db,
                self.mir,
                self.layouts,
                self.context,
                self.names,
                self.module,
                &self.mir.methods[method],
            )
            .run();
        }

        self.setup_classes();
        self.setup_constants();
        self.module.debug_builder.finalize();
    }

    fn setup_classes(&mut self) {
        let mod_id = self.mir.modules[self.module_index].id;
        let space = AddressSpace::default();
        let fn_name = &self.names.setup_classes[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.context, fn_val);
        let entry_block = self.context.append_basic_block(fn_val);

        builder.switch_to_block(entry_block);

        let state_var = builder.alloca(self.layouts.state.ptr_type(space));

        builder.store(state_var, fn_val.get_nth_param(0).unwrap());

        let body = self.context.append_basic_block(fn_val);

        builder.jump(body);
        builder.switch_to_block(body);

        // Allocate all classes defined in this module, and store them in their
        // corresponding globals.
        for &class_id in &self.mir.modules[self.module_index].classes {
            let raw_name = class_id.name(self.db);
            let name_ptr = builder.string_literal(raw_name).0.into();
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
            let class_ptr = match class_id.0 {
                STRING_ID => builder
                    .load_field(self.layouts.state, state, 0)
                    .into_pointer_value(),
                BYTE_ARRAY_ID => builder
                    .load_field(self.layouts.state, state, 1)
                    .into_pointer_value(),
                _ => {
                    let size = builder.int_to_int(
                        self.layouts.instances[&class_id].size_of().unwrap(),
                        32,
                        false,
                    );

                    builder
                        .call(class_new, &[name_ptr, size.into(), methods_len])
                        .into_pointer_value()
                }
            };

            for method in &self.mir.classes[&class_id].methods {
                // Static methods aren't stored in classes, nor can we call them
                // through dynamic dispatch, so we can skip the rest.
                if method.is_static(self.db) {
                    continue;
                }

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
                let var = builder.alloca(self.layouts.method);

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
        let mod_id = self.mir.modules[self.module_index].id;
        let space = AddressSpace::default();
        let fn_name = &self.names.setup_constants[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.context, fn_val);
        let entry_block = self.context.append_basic_block(fn_val);

        builder.switch_to_block(entry_block);

        let state_var = builder.alloca(self.layouts.state.ptr_type(space));

        builder.store(state_var, fn_val.get_nth_param(0).unwrap());

        let body = self.context.append_basic_block(fn_val);

        builder.jump(body);
        builder.switch_to_block(body);

        for &cid in &self.mir.modules[self.module_index].constants {
            let name = &self.names.constants[&cid];
            let global = self.module.add_constant(name);
            let value = &self.mir.constants[&cid];

            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );
            self.set_constant_global(&builder, state_var, value, global);
        }

        for (value, global) in &self.module.strings {
            let ptr = global.as_pointer_value();
            let val = self.new_string(&builder, state_var, value);

            builder.store(ptr, val);
        }

        builder.return_value(None);
    }

    fn set_constant_global(
        &mut self,
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
        &mut self,
        builder: &Builder<'ctx>,
        state_var: PointerValue<'ctx>,
        constant: &Constant,
    ) -> BasicValueEnum<'ctx> {
        match constant {
            Constant::Int(val) => {
                builder.i64_literal(*val).as_basic_value_enum()
            }
            Constant::Float(val) => {
                builder.f64_literal(*val).as_basic_value_enum()
            }
            Constant::String(val) => self.new_string(builder, state_var, val),
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
                    _ => (
                        Shape::Owned,
                        builder.context.pointer_type().as_basic_type_enum(),
                    ),
                };

                let class_id =
                    ClassId::array().specializations(self.db)[&vec![shape]];

                let layout = self.layouts.instances[&class_id];
                let class_name = &self.names.classes[&class_id];
                let class_global = self
                    .module
                    .add_class(class_id, class_name)
                    .as_pointer_value();
                let class = builder.load_untyped_pointer(class_global);
                let alloc =
                    self.module.runtime_function(RuntimeFunction::Allocate);
                let array =
                    builder.call(alloc, &[class.into()]).into_pointer_value();

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
                    let val = self.permanent_value(builder, state_var, arg);

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
        state_var: PointerValue<'ctx>,
        value: &String,
    ) -> BasicValueEnum<'ctx> {
        let state = builder.load_pointer(self.layouts.state, state_var);
        let bytes_typ = builder.context.i8_type().array_type(value.len() as _);
        let bytes_var = builder.alloca(bytes_typ);
        let bytes = builder.string_bytes(value);

        builder.store(bytes_var, bytes);

        let len = builder.u64_literal(value.len() as u64).into();
        let func = self.module.runtime_function(RuntimeFunction::StringNew);

        builder.call(func, &[state.into(), bytes_var.into(), len])
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

    fn run(&mut self) {
        if self.method.id.is_async(self.db) {
            self.async_method();
        } else {
            self.regular_method();
        }
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
                        let proc =
                            self.builder.load_untyped_pointer(proc_var).into();
                        let val =
                            self.builder.load_untyped_pointer(val_var).into();
                        let func_name = RuntimeFunction::ProcessPanic;
                        let func = self.module.runtime_function(func_name);

                        self.builder.call_void(func, &[proc, val]);
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
                    BuiltinFunction::State => {
                        let reg_var = self.variables[&ins.register];
                        let typ = self.layouts.state;
                        let state = self.builder.load_pointer(typ, state_var);

                        self.builder.store(reg_var, state);
                    }
                    BuiltinFunction::Process => {
                        let reg_var = self.variables[&ins.register];
                        let typ = self.layouts.state;
                        let state = self.builder.load_pointer(typ, proc_var);

                        self.builder.store(reg_var, state);
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

                let func_name = ins.method.name(self.db);
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> =
                    Vec::with_capacity(ins.arguments.len() + 1);

                let sret = if let Some(typ) =
                    self.layouts.methods[&ins.method].struct_return
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

                    if self.register_type(ins.register).is_never(self.db) {
                        self.builder.unreachable();
                    }
                }
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
                let func_name = &self.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    self.builder.load(rec_typ, rec_var).into(),
                ];

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
                let info = &self.layouts.methods[&ins.method];
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
                let fn_typ = info.signature;
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

                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    rec.into(),
                ];

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

                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    self.builder
                        .load_pointer(self.layouts.state, state_var)
                        .into(),
                    self.builder.load_untyped_pointer(proc_var).into(),
                    rec.into(),
                ];

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
                let space = AddressSpace::default();
                let sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.layouts.state.ptr_type(space).into(), // State
                    self.builder.context.pointer_type().into(), // Process
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
                let rec_typ = self.variable_types[&ins.receiver];
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

                let state = self
                    .builder
                    .load_pointer(self.layouts.state, state_var)
                    .into();

                let sender = self.builder.load_untyped_pointer(proc_var).into();
                let rec = self.builder.load(rec_typ, rec_var).into();

                self.builder.call_void(
                    send_message,
                    &[state, sender, rec, message.into()],
                );
            }
            Instruction::GetField(ins)
                if ins.class.kind(self.db).is_extern() =>
            {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let layout = self.layouts.instances[&ins.class];
                let index = ins.field.index(self.db) as u32;
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
                if ins.class.kind(self.db).is_extern() =>
            {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let layout = self.layouts.instances[&ins.class];
                let index = ins.field.index(self.db) as u32;
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
                let base = if ins.class.kind(self.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index = (base + ins.field.index(self.db)) as u32;
                let layout = self.layouts.instances[&ins.class];
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
                let base = if ins.class.kind(self.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index = (base + ins.field.index(self.db)) as u32;
                let layout = self.layouts.instances[&ins.class];
                let rec = self.builder.load(rec_typ, rec_var);
                let addr = self.builder.field_address(
                    layout,
                    rec.into_pointer_value(),
                    index,
                );

                self.builder.store(reg_var, addr);
            }
            Instruction::SetField(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let rec_typ = self.variable_types[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let val_typ = self.variable_types[&ins.value];
                let base = if ins.class.kind(self.db).is_async() {
                    PROCESS_FIELD_OFFSET
                } else {
                    FIELD_OFFSET
                };

                let index = (base + ins.field.index(self.db)) as u32;
                let val = self.builder.load(val_typ, val_var);
                let layout = self.layouts.instances[&ins.class];
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
                let func_name = if ins.class.is_atomic(self.db) {
                    RuntimeFunction::AllocateAtomic
                } else {
                    RuntimeFunction::Allocate
                };

                let func = self.module.runtime_function(func_name);
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
                let typ = self.variable_types[&ins.register];
                let name = &self.names.constants[&ins.id];
                let global = self.module.add_constant(name).as_pointer_value();
                let value = self.builder.load(typ, global);

                self.builder.store(var, value);
            }
            Instruction::Preempt(_) => {
                let state = self.builder.load_untyped_pointer(state_var);
                let proc = self.builder.load_untyped_pointer(proc_var);

                // To access the process' epoch we need a process layout. Since
                // we don't care which one as the epoch is in a fixed place, we
                // just use the layout of the main class, which is a process and
                // is always present at this point.
                let layout =
                    self.layouts.instances[&self.db.main_class().unwrap()];

                let state_epoch_addr = self.builder.field_address(
                    self.layouts.state,
                    state,
                    STATE_EPOCH_OFFSET,
                );

                let state_epoch =
                    self.builder.load_atomic_counter(state_epoch_addr);

                let proc_epoch = self
                    .builder
                    .load_field(layout, proc, PROCESS_EPOCH_OFFSET)
                    .into_int_value();

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
            let typ =
                self.builder.context.llvm_type(self.db, self.layouts, raw);

            self.variables.insert(id, self.builder.alloca(typ));
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
        let space = AddressSpace::default();
        let typ = context.i32_type().fn_type(
            &[
                context.i32_type().into(),
                context.i8_type().ptr_type(space).into(),
            ],
            false,
        );
        let function = module.add_function("main", typ, None);
        let builder = Builder::new(context, function);

        GenerateMain { db, mir, layouts, names, context, module, builder }
    }

    fn run(self) {
        let space = AddressSpace::default();
        let entry_block = self.builder.add_block();

        self.builder.switch_to_block(entry_block);

        let argc_typ = self.builder.context.i32_type();
        let argv_typ = self.builder.context.i8_type().ptr_type(space);
        let argc_var = self.builder.alloca(argc_typ);
        let argv_var = self.builder.alloca(argv_typ);

        self.builder.store(argc_var, self.builder.argument(0));
        self.builder.store(argv_var, self.builder.argument(1));

        let argc = self.builder.load(argc_typ, argc_var);
        let argv = self.builder.load(argv_typ, argv_var);
        let layout = self.layouts.method_counts;
        let counts = self.builder.alloca(layout);

        self.set_method_count(counts, ClassId::string());
        self.set_method_count(counts, ClassId::byte_array());

        let rt_new = self.module.runtime_function(RuntimeFunction::RuntimeNew);
        let rt_start =
            self.module.runtime_function(RuntimeFunction::RuntimeStart);
        let rt_state =
            self.module.runtime_function(RuntimeFunction::RuntimeState);
        let rt_drop =
            self.module.runtime_function(RuntimeFunction::RuntimeDrop);
        let runtime = self
            .builder
            .call(rt_new, &[counts.into(), argc.into(), argv.into()])
            .into_pointer_value();
        let state =
            self.builder.call(rt_state, &[runtime.into()]).into_pointer_value();

        // Allocate and store all the classes in their corresponding globals.
        for &id in self.mir.modules.keys() {
            let name = &self.names.setup_classes[&id];
            let func = self.module.add_setup_function(name);

            self.builder.call_void(func, &[state.into()]);
        }

        // Constants need to be defined in a separate pass, as they may depends
        // on the classes (e.g. array constants need the Array class to be set
        // up).
        for &id in self.mir.modules.keys() {
            let name = &self.names.setup_constants[&id];
            let func = self.module.add_setup_function(name);

            self.builder.call_void(func, &[state.into()]);
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
        self.builder.return_value(Some(&self.builder.u32_literal(0)));
    }

    fn methods(&self, id: ClassId) -> IntValue<'ctx> {
        self.context.i16_type().const_int(self.layouts.methods(id) as _, false)
    }

    fn set_method_count(&self, counts: PointerValue<'ctx>, class: ClassId) {
        let layout = self.layouts.method_counts;

        self.builder.store_field(layout, counts, class.0, self.methods(class));
    }
}
