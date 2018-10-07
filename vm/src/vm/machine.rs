//! Virtual Machine for running instructions
use float_cmp::ApproxEqUlps;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use rayon::ThreadPoolBuilder;
use std::env;
use std::f64;
use std::fs;
use std::i32;
use std::i64;
use std::io::{self, Seek, SeekFrom, Write};
use std::ops::{Add, Mul, Sub};
use std::panic;
use std::thread;

use block::Block;
use byte_array;
use compiled_code::CompiledCodePointer;
use date_time::DateTime;
use directories;
use execution_context::ExecutionContext;
use filesystem;
use gc::request::Request as GcRequest;
use hasher::Hasher;
use immix::copy_object::CopyObject;
use integer_operations;
use io::{read_from_stream, ReadResult};
use module_registry::{ModuleRegistry, RcModuleRegistry};
use numeric::division::{FlooredDiv, OverflowingFlooredDiv};
use numeric::modulo::{Modulo, OverflowingModulo};
use object_pointer::ObjectPointer;
use object_value;
use pool::{Job, JoinGuard as PoolJoinGuard, Worker, STACK_SIZE};
use pools::{PRIMARY_POOL, SECONDARY_POOL};
use process::{Process, ProcessStatus, RcProcess};
use runtime_panic;
use slicing;
use stacktrace;
use vm::file_open_mode;
use vm::instruction::{Instruction, InstructionType};
use vm::state::RcState;

macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context = $process.context_mut();
        $index = $context.instruction_index;
    }};
}

macro_rules! remember_and_reset {
    ($process: expr, $context: ident, $index: ident) => {
        $context.instruction_index = $index - 1;

        reset_context!($process, $context, $index);
        continue;
    };
}

macro_rules! throw_value {
    (
        $machine:expr,
        $process:expr,
        $value:expr,
        $context:ident,
        $index:ident
    ) => {{
        $context.instruction_index = $index;

        $machine.throw($process, $value)?;

        reset_context!($process, $context, $index);
    }};
}

macro_rules! throw_error_message {
    (
        $machine:expr,
        $process:expr,
        $message:expr,
        $context:ident,
        $index:ident
    ) => {{
        let value = $process.allocate(
            object_value::string($message),
            $machine.state.string_prototype,
        );

        throw_value!($machine, $process, value, $context, $index);
    }};
}

macro_rules! throw_io_error {
    (
        $machine:expr,
        $process:expr,
        $error:expr,
        $context:ident,
        $index:ident
    ) => {{
        let msg = $crate::error_messages::from_io_error(&$error);

        throw_error_message!($machine, $process, msg, $context, $index);
    }};
}

macro_rules! enter_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context.instruction_index = $index;

        reset_context!($process, $context, $index);
    }};
}

macro_rules! set_nil_if_immutable {
    ($vm:expr, $context:expr, $pointer:expr, $register:expr) => {{
        if $pointer.is_immutable() {
            $context.set_register($register, $vm.state.nil_object);
            continue;
        }
    }};
}

macro_rules! safepoint_and_reduce {
    ($vm:expr, $process:expr, $reductions:expr) => {{
        if $vm.gc_safepoint(&$process) {
            return Ok(());
        }

        // Reduce once we've exhausted all the instructions in a
        // context.
        if $reductions > 0 {
            $reductions -= 1;
        } else {
            $vm.state.process_pools.schedule($process.clone());
            return Ok(());
        }
    }};
}

macro_rules! optional_timeout {
    ($pointer:expr) => {{
        if let Ok(time) = $pointer.integer_value() {
            if time > 0 {
                Some(time as u64)
            } else {
                None
            }
        } else {
            None
        }
    }};
}

macro_rules! create_or_remove_path {
    (
        $vm:expr,
        $instruction:expr,
        $context:expr,
        $op:ident,
        $recursive_op:ident
    ) => {{
        let path_ptr = $context.get_register($instruction.arg(1));
        let rec_ptr = $context.get_register($instruction.arg(2));
        let path = path_ptr.string_value()?;

        if is_false!($vm, rec_ptr) {
            fs::$op(path)
        } else {
            fs::$recursive_op(path)
        }
    }};
}

macro_rules! boolean_to_pointer {
    ($vm:expr, $expr:expr) => {
        if $expr {
            $vm.state.true_object
        } else {
            $vm.state.false_object
        }
    };
}

macro_rules! write_bytes_or_string {
    ($stream:expr, $pointer:expr) => {
        if $pointer.is_string() {
            let buffer = $pointer.string_value()?.as_bytes();

            $stream.write(buffer)
        } else {
            let bytes = $pointer.byte_array_value()?;

            $stream.write(&bytes)
        }
    };
}

#[derive(Clone)]
pub struct Machine {
    pub state: RcState,
    pub module_registry: RcModuleRegistry,
}

impl Machine {
    /// Creates a new Machine with various fields set to their defaults.
    pub fn default(state: RcState) -> Self {
        let module_registry = ModuleRegistry::with_rc(state.clone());

        Machine::new(state, module_registry)
    }

    pub fn new(state: RcState, module_registry: RcModuleRegistry) -> Self {
        Machine {
            state,
            module_registry,
        }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until it returns.
    ///
    /// This method returns true if the VM terminated successfully, false
    /// otherwise.
    pub fn start(&self, file: &str) {
        self.configure_rayon();

        let primary_guard = self.start_primary_threads();
        let gc_pool_guard = self.start_gc_threads();
        let finalizer_pool_guard = self.start_finalizer_threads();
        let secondary_guard = self.start_secondary_threads();
        let suspend_guard = self.start_suspension_worker();

        self.start_main_process(file);

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output.
        if primary_guard.join().is_err()
            || secondary_guard.join().is_err()
            || gc_pool_guard.join().is_err()
            || finalizer_pool_guard.join().is_err()
            || suspend_guard.join().is_err()
        {
            self.state.set_exit_status(1);
        }
    }

    fn configure_rayon(&self) {
        ThreadPoolBuilder::new()
            .thread_name(|idx| format!("rayon {}", idx))
            .num_threads(self.state.config.generic_parallel_threads as usize)
            .stack_size(STACK_SIZE)
            .build_global()
            .unwrap();
    }

    fn start_primary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(PRIMARY_POOL).unwrap();

        pool.run(move |worker, process| {
            machine.run_with_error_handling(worker, &process)
        })
    }

    fn start_secondary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(SECONDARY_POOL).unwrap();

        pool.run(move |worker, process| {
            machine.run_with_error_handling(worker, &process)
        })
    }

    fn start_suspension_worker(&self) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        let builder = thread::Builder::new()
            .stack_size(STACK_SIZE)
            .name("suspend worker".to_string());

        builder
            .spawn(move || {
                state.suspension_list.process_suspended_processes(&state)
            })
            .unwrap()
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> PoolJoinGuard<()> {
        self.state
            .gc_pool
            .run(move |_, mut request| request.perform())
    }

    pub fn start_finalizer_threads(&self) -> PoolJoinGuard<()> {
        self.state
            .finalizer_pool
            .run(move |_, mut block| block.finalize_pending())
    }

    fn terminate(&self) {
        self.state.process_pools.terminate();
        self.state.gc_pool.terminate();
        self.state.finalizer_pool.terminate();
        self.state.suspension_list.terminate();
    }

    /// Starts the main process
    pub fn start_main_process(&self, file: &str) {
        let process = {
            let mut registry = write_lock!(self.module_registry);

            let module = registry
                .parse_module(file)
                .map_err(|err| err.message())
                .unwrap();

            let code = module.code();
            let block = Block::new(
                code,
                None,
                self.state.top_level,
                module.global_scope_ref(),
            );

            self.allocate_process(PRIMARY_POOL, &block).unwrap()
        };

        self.state.process_pools.schedule(process);
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(
        &self,
        pool_id: u8,
        block: &Block,
    ) -> Result<RcProcess, String> {
        let mut process_table = write_lock!(self.state.process_table);

        let pid = process_table
            .reserve()
            .ok_or_else(|| "No PID could be reserved".to_string())?;

        let process = Process::from_block(
            pid,
            pool_id,
            block,
            self.state.global_allocator.clone(),
            &self.state.config,
        );

        process_table.map(pid, process.clone());

        Ok(process)
    }

    /// Executes a single process, terminating in the event of an error.
    pub fn run_with_error_handling(
        &self,
        worker: &Worker,
        process: &RcProcess,
    ) {
        let result = panic::catch_unwind(|| {
            if let Err(message) = self.run(worker, process) {
                self.panic(worker, process, &message);
            }
        });

        if let Err(error) = result {
            if let Ok(message) = error.downcast::<String>() {
                self.panic(worker, process, &message);
            } else {
                self.panic(
                    worker,
                    process,
                    &"The VM panicked with an unknown error",
                );
            };
        }
    }

    /// Executes a single process.
    #[cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]
    pub fn run(
        &self,
        worker: &Worker,
        process: &RcProcess,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions;

        process.running();

        let mut context;
        let mut index;
        let mut instruction;

        reset_context!(process, context, index);

        'exec_loop: loop {
            instruction = unsafe { context.code.instruction(index) };
            index += 1;

            match instruction.instruction_type {
                InstructionType::SetLiteral => {
                    let register = instruction.arg(0);
                    let index = instruction.arg(1);
                    let literal = unsafe { context.code.literal(index) };

                    context.set_register(register, literal);
                }
                InstructionType::SetObject => {
                    let register = instruction.arg(0);
                    let is_permanent_ptr =
                        context.get_register(instruction.arg(1));

                    let is_permanent =
                        is_permanent_ptr != self.state.false_object;

                    let obj = if is_permanent {
                        self.state.permanent_allocator.lock().allocate_empty()
                    } else {
                        process.allocate_empty()
                    };

                    if let Some(proto_index) = instruction.arg_opt(2) {
                        let mut proto = context.get_register(proto_index);

                        if is_permanent && !proto.is_permanent() {
                            proto = self
                                .state
                                .permanent_allocator
                                .lock()
                                .copy_object(proto);
                        }

                        obj.get_mut().set_prototype(proto);
                    }

                    context.set_register(register, obj);
                }
                InstructionType::SetArray => {
                    let register = instruction.arg(0);
                    let val_count = instruction.arguments.len() - 1;

                    let values = self.collect_arguments(
                        &process,
                        &instruction,
                        1,
                        val_count,
                    );

                    let obj = process.allocate(
                        object_value::array(values),
                        self.state.array_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::GetIntegerPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.integer_prototype,
                    );
                }
                InstructionType::GetFloatPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.float_prototype,
                    );
                }
                InstructionType::GetStringPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.string_prototype,
                    );
                }
                InstructionType::GetArrayPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.array_prototype,
                    );
                }
                InstructionType::GetBlockPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.block_prototype,
                    );
                }
                InstructionType::GetObjectPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.object_prototype,
                    );
                }
                InstructionType::GetTrue => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.true_object,
                    );
                }
                InstructionType::GetFalse => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.false_object,
                    );
                }
                InstructionType::SetLocal => {
                    let local_index = instruction.arg(0);
                    let object = context.get_register(instruction.arg(1));

                    context.set_local(local_index, object);
                }
                InstructionType::GetLocal => {
                    let register = instruction.arg(0);
                    let local_index = instruction.arg(1);
                    let object = context.get_local(local_index);

                    context.set_register(register, object);
                }
                InstructionType::SetBlock => {
                    let register = instruction.arg(0);
                    let cc_index = instruction.arg(1);
                    let cc = context.code.code_object(cc_index);

                    let captures_from = if cc.captures {
                        Some(context.binding.clone())
                    } else {
                        None
                    };

                    let receiver = if let Some(rec_reg) = instruction.arg_opt(2)
                    {
                        context.get_register(rec_reg)
                    } else {
                        context.binding.receiver
                    };

                    let block = Block::new(
                        cc,
                        captures_from,
                        receiver,
                        *process.global_scope(),
                    );

                    let obj = process.allocate(
                        object_value::block(block),
                        self.state.block_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::Return => {
                    // If there are any pending deferred blocks, execute these
                    // first, then retry this instruction.
                    if context.schedule_deferred_blocks(process)? {
                        remember_and_reset!(process, context, index);
                    }

                    if context.terminate_upon_return {
                        break 'exec_loop;
                    }

                    let block_return = instruction.arg(0) == 1;

                    let object = if let Some(register) = instruction.arg_opt(1)
                    {
                        context.get_register(register)
                    } else {
                        self.state.nil_object
                    };

                    if block_return {
                        self.unwind_until_defining_scope(process);

                        context = process.context_mut();
                    }

                    if let Some(register) = context.return_register {
                        if let Some(parent_context) = context.parent_mut() {
                            parent_context
                                .set_register(usize::from(register), object);
                        }
                    }

                    // Once we're at the top-level _and_ we have no more
                    // instructions to process we'll bail out of the main
                    // execution loop.
                    if process.pop_context() {
                        break 'exec_loop;
                    }

                    reset_context!(process, context, index);

                    safepoint_and_reduce!(self, process, reductions);
                }
                InstructionType::GotoIfFalse => {
                    let value_reg = instruction.arg(1);

                    if is_false!(self, context.get_register(value_reg)) {
                        index = instruction.arg(0);
                    }
                }
                InstructionType::GotoIfTrue => {
                    let value_reg = instruction.arg(1);

                    if !is_false!(self, context.get_register(value_reg)) {
                        index = instruction.arg(0);
                    }
                }
                InstructionType::Goto => {
                    index = instruction.arg(0);
                }
                InstructionType::IntegerAdd => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        add,
                        overflowing_add
                    );
                }
                InstructionType::IntegerDiv => {
                    let divide_with = context.get_register(instruction.arg(2));

                    if divide_with.is_zero_integer() {
                        return Err("Can not divide an Integer by 0".to_string());
                    }

                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        floored_division,
                        overflowing_floored_division
                    );
                }
                InstructionType::IntegerMul => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        mul,
                        overflowing_mul
                    );
                }
                InstructionType::IntegerSub => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        sub,
                        overflowing_sub
                    );
                }
                InstructionType::IntegerMod => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        modulo,
                        overflowing_modulo
                    );
                }
                InstructionType::IntegerToFloat => {
                    let register = instruction.arg(0);
                    let integer_ptr = context.get_register(instruction.arg(1));

                    let result = if integer_ptr.is_bigint() {
                        integer_ptr.bigint_value().unwrap().to_f64().unwrap()
                    } else {
                        integer_ptr.integer_value()? as f64
                    };

                    let obj = process.allocate(
                        object_value::float(result),
                        self.state.float_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::IntegerToString => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));

                    let result = if rec_ptr.is_integer() {
                        rec_ptr.integer_value()?.to_string()
                    } else if rec_ptr.is_bigint() {
                        rec_ptr.bigint_value()?.to_string()
                    } else {
                        return Err(
                            "IntegerToString can only be used with integers"
                                .to_string(),
                        );
                    };

                    let obj = process.allocate(
                        object_value::string(result),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::IntegerBitwiseAnd => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        &
                    );
                }
                InstructionType::IntegerBitwiseOr => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        |
                    );
                }
                InstructionType::IntegerBitwiseXor => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        ^
                    );
                }
                InstructionType::IntegerShiftLeft => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_left,
                        bigint_shift_left
                    );
                }
                InstructionType::IntegerShiftRight => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_right,
                        bigint_shift_right
                    );
                }
                InstructionType::IntegerSmaller => {
                    integer_bool_op!(self.state, context, instruction, <);
                }
                InstructionType::IntegerGreater => {
                    integer_bool_op!(self.state, context, instruction, >);
                }
                InstructionType::IntegerEquals => {
                    integer_bool_op!(self.state, context, instruction, ==);
                }
                InstructionType::IntegerGreaterOrEqual => {
                    integer_bool_op!(self.state, context, instruction, >=);
                }
                InstructionType::IntegerSmallerOrEqual => {
                    integer_bool_op!(self.state, context, instruction, <=);
                }
                InstructionType::FloatAdd => {
                    float_op!(self.state, process, instruction, +);
                }
                InstructionType::FloatMul => {
                    float_op!(self.state, process, instruction, *);
                }
                InstructionType::FloatDiv => {
                    float_op!(self.state, process, instruction, /);
                }
                InstructionType::FloatSub => {
                    float_op!(self.state, process, instruction, -);
                }
                InstructionType::FloatMod => {
                    float_op!(self.state, process, instruction, %);
                }
                InstructionType::FloatToInteger => {
                    let register = instruction.arg(0);
                    let float_ptr = context.get_register(instruction.arg(1));
                    let float_val = float_ptr.float_value()?;

                    let result = process.allocate_f64_as_i64(
                        float_val,
                        self.state.integer_prototype,
                    )?;

                    context.set_register(register, result);
                }
                InstructionType::FloatToString => {
                    let register = instruction.arg(0);
                    let float_ptr = context.get_register(instruction.arg(1));
                    let result = float_ptr.float_value()?.to_string();

                    let obj = process.allocate(
                        object_value::string(result),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::FloatSmaller => {
                    float_bool_op!(self.state, context, instruction, <);
                }
                InstructionType::FloatGreater => {
                    float_bool_op!(self.state, context, instruction, >);
                }
                InstructionType::FloatEquals => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let arg_ptr = context.get_register(instruction.arg(2));
                    let rec = rec_ptr.float_value()?;
                    let arg = arg_ptr.float_value()?;

                    let boolean = if !rec.is_nan()
                        && !arg.is_nan()
                        && rec.approx_eq_ulps(&arg, 2)
                    {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, boolean);
                }
                InstructionType::FloatGreaterOrEqual => {
                    float_bool_op!(self.state, context, instruction, >=);
                }
                InstructionType::FloatSmallerOrEqual => {
                    float_bool_op!(self.state, context, instruction, <=);
                }
                InstructionType::ArraySet => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));
                    let value_ptr = context.get_register(instruction.arg(3));

                    let vector = array_ptr.array_value_mut()?;
                    let index = slicing::index_for_slice(
                        vector.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = copy_if_permanent!(
                        self.state.permanent_allocator,
                        value_ptr,
                        array_ptr
                    );

                    if index >= vector.len() {
                        vector.resize(index + 1, self.state.nil_object);
                    }

                    vector[index] = value;

                    process.write_barrier(array_ptr, value);

                    context.set_register(register, value);
                }
                InstructionType::ArrayAt => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));
                    let vector = array_ptr.array_value()?;

                    let index = slicing::index_for_slice(
                        vector.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = vector
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| self.state.nil_object);

                    context.set_register(register, value);
                }
                InstructionType::ArrayRemove => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));

                    let vector = array_ptr.array_value_mut()?;
                    let index = slicing::index_for_slice(
                        vector.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = if index >= vector.len() {
                        self.state.nil_object
                    } else {
                        vector.remove(index)
                    };

                    context.set_register(register, value);
                }
                InstructionType::ArrayLength => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let vector = array_ptr.array_value()?;
                    let length = process.allocate_usize(
                        vector.len(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, length);
                }
                InstructionType::ArrayClear => {
                    let array_ptr = context.get_register(instruction.arg(0));

                    array_ptr.array_value_mut()?.clear();
                }
                InstructionType::StringToLower => {
                    let register = instruction.arg(0);
                    let source_ptr = context.get_register(instruction.arg(1));
                    let lower = source_ptr.string_value()?.to_lowercase();

                    let obj = process.allocate(
                        object_value::string(lower),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::StringToUpper => {
                    let register = instruction.arg(0);
                    let source_ptr = context.get_register(instruction.arg(1));
                    let upper = source_ptr.string_value()?.to_uppercase();

                    let obj = process.allocate(
                        object_value::string(upper),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::StringEquals => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let arg_ptr = context.get_register(instruction.arg(2));

                    let boolean = if rec_ptr.is_interned_string()
                        && arg_ptr.is_interned_string()
                    {
                        if rec_ptr == arg_ptr {
                            self.state.true_object
                        } else {
                            self.state.false_object
                        }
                    } else if rec_ptr.string_value()?
                        == arg_ptr.string_value()?
                    {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, boolean);
                }
                InstructionType::StringToByteArray => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));
                    let bytes = string_ptr.string_value()?.as_bytes().to_vec();

                    let obj = process.allocate(
                        object_value::byte_array(bytes),
                        self.state.object_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::StringLength => {
                    let register = instruction.arg(0);
                    let arg_ptr = context.get_register(instruction.arg(1));
                    let length = process.allocate_usize(
                        arg_ptr.string_value()?.chars().count(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, length);
                }
                InstructionType::StringSize => {
                    let register = instruction.arg(0);
                    let arg_ptr = context.get_register(instruction.arg(1));
                    let size = process.allocate_usize(
                        arg_ptr.string_value()?.len(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, size);
                }
                InstructionType::StdoutWrite => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));
                    let mut stdout = io::stdout();

                    let result = write_bytes_or_string!(stdout, string_ptr)
                        .and_then(|size| stdout.flush().and_then(|_| Ok(size)));

                    match result {
                        Ok(size) => {
                            let obj = process.allocate_usize(
                                size,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::StdoutFlush => {
                    let register = instruction.arg(0);

                    match io::stdout().flush() {
                        Ok(_) => context
                            .set_register(register, self.state.nil_object),
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index)
                        }
                    };
                }
                InstructionType::StderrWrite => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));
                    let mut stderr = io::stderr();

                    let result = write_bytes_or_string!(stderr, string_ptr)
                        .and_then(|size| stderr.flush().and_then(|_| Ok(size)));

                    match result {
                        Ok(size) => {
                            let obj = process.allocate_usize(
                                size,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::StderrFlush => {
                    let register = instruction.arg(0);

                    match io::stderr().flush() {
                        Ok(_) => context
                            .set_register(register, self.state.nil_object),
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index)
                        }
                    };
                }
                InstructionType::StdinRead => {
                    let register = instruction.arg(0);
                    let buff_ptr = context.get_register(instruction.arg(1));
                    let max_bytes = context.get_register(instruction.arg(2));

                    let mut stdin = io::stdin();
                    let mut buffer = buff_ptr.byte_array_value_mut()?;

                    match read_from_stream(&mut stdin, &mut buffer, max_bytes) {
                        ReadResult::Ok(amount) => {
                            let size_ptr = process.allocate_usize(
                                amount,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, size_ptr);
                        }
                        ReadResult::Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                        ReadResult::Panic(message) => return Err(message),
                    }
                }
                InstructionType::FileOpen => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let mode_ptr = context.get_register(instruction.arg(2));

                    let path = path_ptr.string_value()?;
                    let mode = mode_ptr.integer_value()?;
                    let open_opts = file_open_mode::options_for_integer(mode)?;

                    match open_opts.open(path) {
                        Ok(file) => {
                            let obj = process.allocate(
                                object_value::file(file),
                                self.state.object_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                    }
                }
                InstructionType::FileWrite => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let value_ptr = context.get_register(instruction.arg(2));

                    let file = file_ptr.file_value_mut()?;

                    match write_bytes_or_string!(file, value_ptr) {
                        Ok(num_bytes) => {
                            let obj = process.allocate_usize(
                                num_bytes,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                    }
                }
                InstructionType::FileRead => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let buff_ptr = context.get_register(instruction.arg(2));
                    let max_bytes = context.get_register(instruction.arg(3));

                    let mut file = file_ptr.file_value_mut()?;
                    let mut buffer = buff_ptr.byte_array_value_mut()?;

                    match read_from_stream(&mut file, &mut buffer, max_bytes) {
                        ReadResult::Ok(amount) => {
                            let size_ptr = process.allocate_usize(
                                amount,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, size_ptr);
                        }
                        ReadResult::Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                        ReadResult::Panic(message) => return Err(message),
                    }
                }
                InstructionType::FileFlush => {
                    let file_ptr = context.get_register(instruction.arg(0));
                    let file = file_ptr.file_value_mut()?;

                    if let Err(err) = file.flush() {
                        throw_io_error!(self, process, err, context, index);
                    }
                }
                InstructionType::FileSize => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let path = path_ptr.string_value()?;

                    match fs::metadata(path) {
                        Ok(meta) => {
                            let obj = process.allocate_u64(
                                meta.len(),
                                self.state.integer_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                    }
                }
                InstructionType::FileSeek => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let offset_ptr = context.get_register(instruction.arg(2));
                    let file = file_ptr.file_value_mut()?;

                    let offset = if offset_ptr.is_bigint() {
                        let big_offset = offset_ptr.bigint_value()?;

                        if let Some(offset) = big_offset.to_u64() {
                            offset
                        } else {
                            return Err(format!(
                                "{} is too big for a seek offset",
                                big_offset
                            ));
                        }
                    } else {
                        let offset = offset_ptr.integer_value()?;

                        if offset < 0 {
                            return Err(format!(
                                "{} is not a valid seek offset",
                                offset
                            ));
                        }

                        offset as u64
                    };

                    match file.seek(SeekFrom::Start(offset)) {
                        Ok(cursor) => {
                            let obj = process.allocate_u64(
                                cursor,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index);
                        }
                    }
                }
                InstructionType::LoadModule => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let path_str = path_ptr.string_value()?;

                    let (block, execute) = {
                        let mut registry = write_lock!(self.module_registry);

                        let lookup = registry
                            .get_or_set(path_str)
                            .map_err(|err| err.message())?;

                        let module = lookup.module;

                        let block = Block::new(
                            module.code(),
                            None,
                            self.state.top_level,
                            module.global_scope_ref(),
                        );

                        (block, lookup.parsed)
                    };

                    if execute {
                        let new_context = ExecutionContext::from_block(
                            &block,
                            Some(register as u16),
                        );

                        process.push_context(new_context);

                        enter_context!(process, context, index);
                    } else {
                        context.set_register(register, self.state.nil_object);
                    }
                }
                InstructionType::SetAttribute => {
                    let register = instruction.arg(0);
                    let target_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let value_ptr = context.get_register(instruction.arg(3));

                    set_nil_if_immutable!(self, context, target_ptr, register);

                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| {
                            copy_if_permanent!(
                                self.state.permanent_allocator,
                                name_ptr,
                                target_ptr
                            )
                        });

                    let value = copy_if_permanent!(
                        self.state.permanent_allocator,
                        value_ptr,
                        target_ptr
                    );

                    target_ptr.add_attribute(&process, name, value);

                    context.set_register(register, value);
                }
                InstructionType::SetAttributeToObject => {
                    let register = instruction.arg(0);
                    let obj_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));

                    set_nil_if_immutable!(self, context, obj_ptr, register);

                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| {
                            copy_if_permanent!(
                                self.state.permanent_allocator,
                                name_ptr,
                                obj_ptr
                            )
                        });

                    let attribute = if let Some(ptr) =
                        obj_ptr.get().lookup_attribute_in_self(name)
                    {
                        ptr
                    } else {
                        let value = object_value::none();
                        let proto = self.state.object_prototype;

                        let ptr = if obj_ptr.is_permanent() {
                            self.state
                                .permanent_allocator
                                .lock()
                                .allocate_with_prototype(value, proto)
                        } else {
                            process.allocate(value, proto)
                        };

                        obj_ptr.add_attribute(&process, name, ptr);

                        ptr
                    };

                    context.set_register(register, attribute);
                }
                InstructionType::GetAttribute => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));

                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| name_ptr);

                    let method = rec_ptr
                        .lookup_attribute(&self.state, name)
                        .unwrap_or_else(|| self.state.nil_object);

                    context.set_register(register, method);
                }
                InstructionType::SetPrototype => {
                    let source = context.get_register(instruction.arg(0));
                    let proto = context.get_register(instruction.arg(1));

                    source.get_mut().set_prototype(proto);
                }
                InstructionType::GetPrototype => {
                    let register = instruction.arg(0);
                    let source = context.get_register(instruction.arg(1));

                    let proto = source
                        .prototype(&self.state)
                        .unwrap_or_else(|| self.state.nil_object);

                    context.set_register(register, proto);
                }
                InstructionType::LocalExists => {
                    let register = instruction.arg(0);
                    let local_index = instruction.arg(1);

                    let value = if process.local_exists(local_index) {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, value);
                }
                InstructionType::ProcessSpawn => {
                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));

                    let pool_id = if let Some(reg) = instruction.arg_opt(2) {
                        context.get_register(reg).u8_value()?
                    } else {
                        PRIMARY_POOL
                    };

                    let block_obj = block_ptr.block_value()?;
                    let new_proc = self.allocate_process(pool_id, block_obj)?;
                    let new_pid = new_proc.pid;
                    let pid_ptr = new_proc
                        .allocate_usize(new_pid, self.state.integer_prototype);

                    self.state.process_pools.schedule(new_proc);

                    context.set_register(register, pid_ptr);
                }
                InstructionType::ProcessSendMessage => {
                    let register = instruction.arg(0);
                    let pid_ptr = context.get_register(instruction.arg(1));
                    let msg_ptr = context.get_register(instruction.arg(2));
                    let pid = pid_ptr.usize_value()?;

                    if let Some(receiver) =
                        read_lock!(self.state.process_table).get(pid)
                    {
                        receiver.send_message(&process, msg_ptr);

                        if receiver.is_waiting_for_message() {
                            self.state.suspension_list.wake_up();
                        }
                    }

                    context.set_register(register, msg_ptr);
                }
                InstructionType::ProcessReceiveMessage => {
                    let register = instruction.arg(0);

                    if let Some(msg_ptr) = process.receive_message() {
                        context.set_register(register, msg_ptr);
                    } else {
                        let time_ptr = context.get_register(instruction.arg(1));
                        let timeout = optional_timeout!(time_ptr);

                        // When resuming (except when the timeout expires) we
                        // want to retry this instruction so we can store the
                        // received message in the target register.
                        context.instruction_index = index - 1;

                        // If the timeout expires we won't retry this
                        // instruction so we need to ensure the register is
                        // already set.
                        context.set_register(register, self.state.nil_object);

                        process.waiting_for_message();

                        self.state
                            .suspension_list
                            .suspend(process.clone(), timeout);

                        return Ok(());
                    }
                }
                InstructionType::ProcessCurrentPid => {
                    let register = instruction.arg(0);
                    let pid = process.allocate_usize(
                        process.pid,
                        self.state.integer_prototype,
                    );

                    context.set_register(register, pid);
                }
                InstructionType::ProcessStatus => {
                    let register = instruction.arg(0);
                    let pid_ptr = process.get_register(instruction.arg(1));
                    let pid = pid_ptr.usize_value()?;
                    let table = read_lock!(self.state.process_table);

                    let status = if let Some(receiver) = table.get(pid) {
                        receiver.status_integer()
                    } else {
                        ProcessStatus::Finished as u8
                    };

                    let status_ptr = ObjectPointer::integer(i64::from(status));

                    context.set_register(register, status_ptr);
                }
                InstructionType::ProcessSuspendCurrent => {
                    let time_ptr = context.get_register(instruction.arg(0));
                    let timeout = optional_timeout!(time_ptr);

                    context.instruction_index = index;

                    process.suspended();

                    self.state
                        .suspension_list
                        .suspend(process.clone(), timeout);

                    return Ok(());
                }
                InstructionType::SetParentLocal => {
                    let index = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let value = context.get_register(instruction.arg(2));

                    if let Some(binding) = context.binding.find_parent(depth) {
                        binding.set_local(index, value);
                    } else {
                        return Err(format!("No binding for depth {}", depth));
                    }
                }
                InstructionType::GetParentLocal => {
                    let reg = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let index = instruction.arg(2);

                    if let Some(binding) = context.binding.find_parent(depth) {
                        context.set_register(reg, binding.get_local(index));
                    } else {
                        return Err(format!("No binding for depth {}", depth));
                    }
                }
                InstructionType::ObjectEquals => {
                    let register = instruction.arg(0);
                    let compare = context.get_register(instruction.arg(1));
                    let compare_with = context.get_register(instruction.arg(2));

                    let obj = if compare == compare_with {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, obj);
                }
                InstructionType::ObjectIsKindOf => {
                    let register = instruction.arg(0);
                    let compare = context.get_register(instruction.arg(1));
                    let compare_with = context.get_register(instruction.arg(2));

                    let result =
                        if compare.is_kind_of(&self.state, compare_with) {
                            self.state.true_object
                        } else {
                            self.state.false_object
                        };

                    context.set_register(register, result);
                }
                InstructionType::PrototypeChainAttributeContains => {
                    let register = instruction.arg(0);
                    let obj_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let val_ptr = context.get_register(instruction.arg(3));

                    let mut source = obj_ptr;
                    let mut result = self.state.false_object;

                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| name_ptr);

                    // For every object in the prototype chain (including self)
                    // we look up the target object, then we check if the value
                    // is in said object.
                    loop {
                        if let Some(obj) =
                            source.lookup_attribute_in_self(&self.state, name)
                        {
                            if obj
                                .lookup_attribute(&self.state, val_ptr)
                                .is_some()
                            {
                                result = self.state.true_object;
                                break;
                            }
                        }

                        if let Some(proto) = source.prototype(&self.state) {
                            source = proto;
                        } else {
                            break;
                        }
                    }

                    context.set_register(register, result);
                }
                InstructionType::GetToplevel => {
                    context
                        .set_register(instruction.arg(0), self.state.top_level);
                }
                InstructionType::GetNil => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.nil_object,
                    );
                }
                InstructionType::AttributeExists => {
                    let register = instruction.arg(0);
                    let source_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));

                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| name_ptr);

                    let obj = if source_ptr
                        .lookup_attribute(&self.state, name)
                        .is_some()
                    {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, obj);
                }
                InstructionType::RemoveAttribute => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self
                        .state
                        .intern_pointer(name_ptr)
                        .unwrap_or_else(|_| name_ptr);

                    set_nil_if_immutable!(self, context, rec_ptr, register);

                    let obj = if let Some(attribute) =
                        rec_ptr.get_mut().remove_attribute(name)
                    {
                        attribute
                    } else {
                        self.state.nil_object
                    };

                    context.set_register(register, obj);
                }
                InstructionType::GetAttributeNames => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let attributes = rec_ptr.attribute_names();

                    let obj = process.allocate(
                        object_value::array(attributes),
                        self.state.array_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::TimeMonotonic => {
                    let register = instruction.arg(0);
                    let duration = self.state.start_time.elapsed();
                    let seconds = duration.as_secs() as f64
                        + (f64::from(duration.subsec_nanos())
                            / 1_000_000_000.0);

                    let pointer = process.allocate(
                        object_value::float(seconds),
                        self.state.float_prototype,
                    );

                    context.set_register(register, pointer);
                }
                InstructionType::RunBlock => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let block = block_ptr.block_value()?;

                    let new_ctx = ExecutionContext::from_block(
                        &block,
                        Some(register as u16),
                    );

                    self.prepare_new_context(
                        process,
                        &instruction,
                        &new_ctx,
                        instruction.arg(2),
                        instruction.arg(3),
                        4,
                    )?;

                    process.push_context(new_ctx);

                    enter_context!(process, context, index);
                }
                InstructionType::SetGlobal => {
                    let register = instruction.arg(0);
                    let index = instruction.arg(1);
                    let object = context.get_register(instruction.arg(2));

                    let value = if object.is_permanent() {
                        object
                    } else {
                        self.state
                            .permanent_allocator
                            .lock()
                            .copy_object(object)
                    };

                    process.set_global(index, value);
                    context.set_register(register, value);
                }
                InstructionType::GetGlobal => {
                    let register = instruction.arg(0);
                    let index = instruction.arg(1);
                    let object = process.get_global(index);

                    context.set_register(register, object);
                }
                InstructionType::Throw => {
                    let value = context.get_register(instruction.arg(0));

                    throw_value!(self, process, value, context, index);
                }
                InstructionType::SetRegister => {
                    let value = context.get_register(instruction.arg(1));

                    context.set_register(instruction.arg(0), value);
                }
                InstructionType::TailCall => {
                    context.binding.locals_mut().reset();

                    self.prepare_new_context(
                        process,
                        &instruction,
                        context,
                        instruction.arg(0),
                        instruction.arg(1),
                        2,
                    )?;

                    context.register.values.reset();

                    context.instruction_index = 0;

                    reset_context!(process, context, index);

                    safepoint_and_reduce!(self, process, reductions);
                }
                InstructionType::CopyBlocks => {
                    let obj_ptr = context.get_register(instruction.arg(0));
                    let to_impl_ptr = context.get_register(instruction.arg(1));

                    if obj_ptr.is_immutable() || to_impl_ptr.is_immutable() {
                        // When using immutable objects there's nothing to copy
                        // over so we'll just skip over the work.
                        continue;
                    }

                    let mut object = obj_ptr.get_mut();
                    let to_impl = to_impl_ptr.get();

                    if let Some(map) = to_impl.attributes_map() {
                        for (key, val) in map.iter() {
                            if val.block_value().is_err() {
                                continue;
                            }

                            let block = copy_if_permanent!(
                                self.state.permanent_allocator,
                                *val,
                                obj_ptr
                            );

                            object.add_attribute(*key, block);
                        }
                    }
                }
                InstructionType::FloatIsNan => {
                    let register = instruction.arg(0);
                    let pointer = context.get_register(instruction.arg(1));

                    let is_nan = match pointer.float_value() {
                        Ok(float) => float.is_nan(),
                        Err(_) => false,
                    };

                    let result = if is_nan {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, result);
                }
                InstructionType::FloatIsInfinite => {
                    let register = instruction.arg(0);
                    let pointer = context.get_register(instruction.arg(1));

                    let is_inf = match pointer.float_value() {
                        Ok(float) => float.is_infinite(),
                        Err(_) => false,
                    };

                    let result = if is_inf {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, result);
                }
                InstructionType::FloatFloor => {
                    let register = instruction.arg(0);
                    let pointer = context.get_register(instruction.arg(1));
                    let float = pointer.float_value()?.floor();

                    context.set_register(
                        register,
                        process.allocate(
                            object_value::float(float),
                            self.state.float_prototype,
                        ),
                    );
                }
                InstructionType::FloatCeil => {
                    let register = instruction.arg(0);
                    let pointer = context.get_register(instruction.arg(1));
                    let float = pointer.float_value()?.ceil();

                    context.set_register(
                        register,
                        process.allocate(
                            object_value::float(float),
                            self.state.float_prototype,
                        ),
                    );
                }
                InstructionType::FloatRound => {
                    let register = instruction.arg(0);
                    let pointer = context.get_register(instruction.arg(1));
                    let prec_ptr = context.get_register(instruction.arg(2));

                    let precision = prec_ptr.integer_value()?;
                    let float = pointer.float_value()?;

                    let result = if precision == 0 {
                        float.round()
                    } else if precision >= i64::from(i32::MIN)
                        && precision <= i64::from(i32::MAX)
                    {
                        let power = 10.0_f64.powi(precision as i32);
                        let multiplied = float * power;

                        // Certain very large numbers (e.g. f64::MAX) would
                        // produce Infinity when multiplied with the power. In
                        // this case we just return the input float directly.
                        if multiplied.is_finite() {
                            multiplied.round() / power
                        } else {
                            float
                        }
                    } else {
                        float
                    };

                    context.set_register(
                        register,
                        process.allocate(
                            object_value::float(result),
                            self.state.float_prototype,
                        ),
                    );
                }
                InstructionType::Drop => {
                    let pointer = context.get_register(instruction.arg(0));
                    let object = pointer.get_mut();

                    if object.value.is_some() {
                        drop(object.value.take());

                        if !object.has_attributes() {
                            pointer.unmark_for_finalization();
                        }
                    }
                }
                InstructionType::MoveToPool => {
                    let pool_ptr = context.get_register(instruction.arg(0));
                    let pool_id = pool_ptr.u8_value()?;

                    if !self.state.process_pools.pool_id_is_valid(pool_id) {
                        return Err(format!(
                            "The process pool ID {} is invalid",
                            pool_id
                        ));
                    }

                    if process.thread_id().is_some() {
                        // If a process is pinned we can't move it to another
                        // pool. We can't panic in this case, since it would
                        // prevent code from using certain IO operations that
                        // may try to move the process to another pool.
                        //
                        // Instead, we simply ignore the request and continue
                        // running on the current thread.
                        continue;
                    }

                    if pool_id != process.pool_id() {
                        process.set_pool_id(pool_id);

                        context.instruction_index = index;

                        // After this we can _not_ perform any operations on the
                        // process any more as it might be concurrently modified
                        // by the pool we just moved it to.
                        self.state.process_pools.schedule(process.clone());

                        return Ok(());
                    }
                }
                InstructionType::FileRemove => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let path_str = path_ptr.string_value()?;

                    match fs::remove_file(path_str) {
                        Ok(_) => context
                            .set_register(register, self.state.nil_object),
                        Err(err) => {
                            throw_io_error!(self, process, err, context, index)
                        }
                    };
                }
                InstructionType::Panic => {
                    context.line = instruction.line;

                    let message_ptr = context.get_register(instruction.arg(0));

                    return Err(message_ptr.string_value()?.clone());
                }
                InstructionType::Exit => {
                    // Any pending deferred blocks should be executed first.
                    if context
                        .schedule_deferred_blocks_of_all_parents(process)?
                    {
                        remember_and_reset!(process, context, index);
                    }

                    let status_ptr = context.get_register(instruction.arg(0));
                    let status = status_ptr.i32_value()?;

                    self.state.set_exit_status(status);
                    self.terminate();

                    return Ok(());
                }
                InstructionType::Platform => {
                    let register = instruction.arg(0);

                    let platform = if cfg!(windows) {
                        0
                    } else if cfg!(unix) {
                        1
                    } else {
                        2
                    };

                    context.set_register(
                        register,
                        ObjectPointer::integer(platform),
                    );
                }
                InstructionType::FileCopy => {
                    let register = instruction.arg(0);
                    let src_ptr = context.get_register(instruction.arg(1));
                    let dst_ptr = context.get_register(instruction.arg(2));
                    let src = src_ptr.string_value()?;
                    let dst = dst_ptr.string_value()?;

                    match fs::copy(src, dst) {
                        Ok(bytes) => {
                            let pointer = process.allocate_u64(
                                bytes,
                                self.state.integer_prototype,
                            );

                            context.set_register(register, pointer);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self, process, error, context, index
                            );
                        }
                    };
                }
                InstructionType::FileType => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let path = path_ptr.string_value()?;
                    let file_type = filesystem::type_of_path(path);

                    context.set_register(
                        register,
                        ObjectPointer::integer(file_type),
                    );
                }
                InstructionType::FileTime => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let kind_ptr = context.get_register(instruction.arg(2));

                    let path = path_ptr.string_value()?;
                    let kind = kind_ptr.integer_value()?;

                    match filesystem::date_time_for_path(path, kind) {
                        Ok(dt) => {
                            let ptr = process.allocate(
                                object_value::float(dt.timestamp()),
                                self.state.float_prototype,
                            );

                            context.set_register(register, ptr);
                        }
                        Err(error) => {
                            throw_error_message!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::TimeSystem => {
                    let register = instruction.arg(0);
                    let timestamp = DateTime::now().timestamp();
                    let pointer = process.allocate(
                        object_value::float(timestamp),
                        self.state.float_prototype,
                    );

                    context.set_register(register, pointer);
                }
                InstructionType::TimeSystemOffset => {
                    let register = instruction.arg(0);
                    let offset =
                        ObjectPointer::integer(DateTime::now().utc_offset());

                    context.set_register(register, offset);
                }
                InstructionType::TimeSystemDst => {
                    let register = instruction.arg(0);
                    let result = if DateTime::now().dst_active() {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, result);
                }
                InstructionType::DirectoryCreate => {
                    let register = instruction.arg(0);
                    let result = create_or_remove_path!(
                        self,
                        instruction,
                        context,
                        create_dir,
                        create_dir_all
                    );

                    if let Err(error) = result {
                        throw_io_error!(self, process, error, context, index);
                    } else {
                        context.set_register(register, self.state.nil_object);
                    }
                }
                InstructionType::DirectoryRemove => {
                    let register = instruction.arg(0);
                    let result = create_or_remove_path!(
                        self,
                        instruction,
                        context,
                        remove_dir,
                        remove_dir_all
                    );

                    if let Err(error) = result {
                        throw_io_error!(self, process, error, context, index);
                    } else {
                        context.set_register(register, self.state.nil_object);
                    }
                }
                InstructionType::DirectoryList => {
                    let register = instruction.arg(0);
                    let path_ptr = context.get_register(instruction.arg(1));
                    let path = path_ptr.string_value()?;

                    match filesystem::list_directory_as_pointers(
                        &self.state,
                        process,
                        path,
                    ) {
                        Ok(array) => {
                            context.set_register(register, array);
                        }
                        Err(error) => {
                            throw_error_message!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::StringConcat => {
                    let register = instruction.arg(0);
                    let left = context.get_register(instruction.arg(1));
                    let right = context.get_register(instruction.arg(2));
                    let result =
                        left.string_value()?.clone() + right.string_value()?;

                    let pointer = process.allocate(
                        object_value::string(result),
                        self.state.string_prototype,
                    );

                    context.set_register(register, pointer);
                }
                InstructionType::HasherNew => {
                    let register = instruction.arg(0);
                    let pointer = process.allocate(
                        object_value::hasher(Hasher::new()),
                        self.state.object_prototype,
                    );

                    context.set_register(register, pointer);
                }
                InstructionType::HasherWrite => {
                    let register = instruction.arg(0);
                    let mut hasher_ptr =
                        context.get_register(instruction.arg(1));

                    let val_ptr = context.get_register(instruction.arg(2));

                    val_ptr.hash_object(hasher_ptr.hasher_value_mut()?)?;

                    context.set_register(register, self.state.nil_object);
                }
                InstructionType::HasherFinish => {
                    let register = instruction.arg(0);
                    let mut hasher_ptr =
                        context.get_register(instruction.arg(1));

                    let result = hasher_ptr.hasher_value_mut()?.finish();

                    let pointer = if ObjectPointer::integer_too_large(result) {
                        process.allocate(
                            object_value::integer(result),
                            self.state.integer_prototype,
                        )
                    } else {
                        ObjectPointer::integer(result)
                    };

                    context.set_register(register, pointer);
                }
                InstructionType::Stacktrace => {
                    let register = instruction.arg(0);
                    let limit_ptr = context.get_register(instruction.arg(1));
                    let skip_ptr = context.get_register(instruction.arg(2));

                    let limit = if limit_ptr == self.state.nil_object {
                        None
                    } else {
                        Some(limit_ptr.usize_value()?)
                    };

                    let skip = skip_ptr.usize_value()?;

                    let array = stacktrace::allocate_stacktrace(
                        process,
                        &self.state,
                        limit,
                        skip,
                    );

                    context.set_register(register, array);
                }
                InstructionType::ProcessTerminateCurrent => {
                    break 'exec_loop;
                }
                InstructionType::StringSlice => {
                    let register = instruction.arg(0);
                    let str_ptr = context.get_register(instruction.arg(1));
                    let start_ptr = context.get_register(instruction.arg(2));
                    let amount_ptr = context.get_register(instruction.arg(3));

                    let string = str_ptr.string_value()?;
                    let amount = amount_ptr.usize_value()?;

                    let start = slicing::index_for_slice(
                        string.chars().count(),
                        start_ptr.integer_value()?,
                    );

                    let new_string = string
                        .chars()
                        .skip(start)
                        .take(amount)
                        .collect::<String>();

                    let new_string_ptr = process.allocate(
                        object_value::string(new_string),
                        self.state.string_prototype,
                    );

                    context.set_register(register, new_string_ptr);
                }
                InstructionType::BlockMetadata => {
                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let field_ptr = context.get_register(instruction.arg(2));

                    let block = block_ptr.block_value()?;
                    let kind = field_ptr.integer_value()?;

                    let result = match kind {
                        0 => block.code.name,
                        1 => block.code.file,
                        2 => ObjectPointer::integer(i64::from(block.code.line)),
                        3 => process.allocate(
                            object_value::array(block.code.arguments.clone()),
                            self.state.array_prototype,
                        ),
                        4 => ObjectPointer::integer(i64::from(
                            block.code.required_arguments,
                        )),
                        5 => {
                            boolean_to_pointer!(self, block.code.rest_argument)
                        }
                        _ => {
                            return Err(format!(
                                "{} is not a valid block metadata type",
                                kind
                            ));
                        }
                    };

                    context.set_register(register, result);
                }
                InstructionType::StringFormatDebug => {
                    let register = instruction.arg(0);
                    let str_ptr = context.get_register(instruction.arg(1));
                    let new_str = format!("{:?}", str_ptr.string_value()?);

                    let new_str_ptr = process.allocate(
                        object_value::string(new_str),
                        self.state.string_prototype,
                    );

                    context.set_register(register, new_str_ptr);
                }
                InstructionType::StringConcatMultiple => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let array = array_ptr.array_value()?;
                    let mut buffer = String::new();

                    for str_ptr in array.iter() {
                        buffer.push_str(str_ptr.string_value()?);
                    }

                    let new_str_ptr = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, new_str_ptr);
                }
                InstructionType::ByteArrayFromArray => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));

                    let integers = array_ptr.array_value()?;
                    let mut bytes = Vec::with_capacity(integers.len());

                    for value in integers.iter() {
                        bytes.push(byte_array::integer_to_byte(*value)?);
                    }

                    let pointer = process.allocate(
                        object_value::byte_array(bytes),
                        self.state.object_prototype,
                    );

                    context.set_register(register, pointer);
                }
                InstructionType::ByteArraySet => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));
                    let value_ptr = context.get_register(instruction.arg(3));

                    let bytes = array_ptr.byte_array_value_mut()?;
                    let index = slicing::index_for_slice(
                        bytes.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = byte_array::integer_to_byte(value_ptr)?;

                    if index > bytes.len() {
                        return Err(format!(
                            "Byte array index {} is out of bounds",
                            index
                        ));
                    }

                    if index == bytes.len() {
                        bytes.push(value);
                    } else {
                        bytes[index] = value;
                    }

                    context.set_register(register, value_ptr);
                }
                InstructionType::ByteArrayAt => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));
                    let bytes = array_ptr.byte_array_value()?;

                    let index = slicing::index_for_slice(
                        bytes.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = bytes
                        .get(index)
                        .map(|byte| ObjectPointer::byte(*byte))
                        .unwrap_or_else(|| self.state.nil_object);

                    context.set_register(register, value);
                }
                InstructionType::ByteArrayRemove => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));

                    let bytes = array_ptr.byte_array_value_mut()?;
                    let index = slicing::index_for_slice(
                        bytes.len(),
                        index_ptr.integer_value()?,
                    );

                    let value = if index >= bytes.len() {
                        self.state.nil_object
                    } else {
                        ObjectPointer::byte(bytes.remove(index))
                    };

                    context.set_register(register, value);
                }
                InstructionType::ByteArrayLength => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let bytes = array_ptr.byte_array_value()?;
                    let length = process.allocate_usize(
                        bytes.len(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, length);
                }
                InstructionType::ByteArrayClear => {
                    let array_ptr = context.get_register(instruction.arg(0));

                    array_ptr.byte_array_value_mut()?.clear();
                }
                InstructionType::ByteArrayEquals => {
                    let register = instruction.arg(0);
                    let compare_ptr = context.get_register(instruction.arg(1));
                    let compare_with_ptr =
                        context.get_register(instruction.arg(2));

                    let result = if compare_ptr.byte_array_value()?
                        == compare_with_ptr.byte_array_value()?
                    {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, result);
                }
                InstructionType::ByteArrayToString => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let drain_ptr = context.get_register(instruction.arg(2));
                    let mut input_bytes = array_ptr.byte_array_value_mut()?;

                    let mut string_bytes =
                        if drain_ptr == self.state.true_object {
                            input_bytes.drain(0..).collect()
                        } else {
                            input_bytes.clone()
                        };

                    let string = match String::from_utf8(string_bytes) {
                        Ok(string) => string,
                        Err(err) => String::from_utf8_lossy(&err.into_bytes())
                            .into_owned(),
                    };

                    let obj = process.allocate(
                        object_value::string(string),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                InstructionType::GetBooleanPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.boolean_prototype,
                    );
                }
                InstructionType::EnvGet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let var_name = var.string_value()?;

                    let val = if let Some(val) = env::var_os(var_name) {
                        let string = val.to_string_lossy().into_owned();

                        process.allocate(
                            object_value::string(string),
                            self.state.string_prototype,
                        )
                    } else {
                        self.state.nil_object
                    };

                    context.set_register(reg, val);
                }
                InstructionType::EnvSet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = context.get_register(instruction.arg(2));

                    env::set_var(var.string_value()?, val.string_value()?);

                    context.set_register(reg, val);
                }
                InstructionType::EnvVariables => {
                    let reg = instruction.arg(0);
                    let names = env::vars_os()
                        .map(|(key, _)| {
                            process.allocate(
                                object_value::string(
                                    key.to_string_lossy().into_owned(),
                                ),
                                self.state.string_prototype,
                            )
                        })
                        .collect();

                    let array = process.allocate(
                        object_value::array(names),
                        self.state.array_prototype,
                    );

                    context.set_register(reg, array);
                }
                InstructionType::EnvHomeDirectory => {
                    let reg = instruction.arg(0);

                    let path = if let Some(path) = directories::home() {
                        process.allocate(
                            object_value::string(path),
                            self.state.string_prototype,
                        )
                    } else {
                        self.state.nil_object
                    };

                    context.set_register(reg, path);
                }
                InstructionType::EnvTempDirectory => {
                    let reg = instruction.arg(0);

                    let path = process.allocate(
                        object_value::string(directories::temp()),
                        self.state.string_prototype,
                    );

                    context.set_register(reg, path);
                }
                InstructionType::EnvGetWorkingDirectory => {
                    let reg = instruction.arg(0);

                    match directories::working_directory() {
                        Ok(path_string) => {
                            let path = process.allocate(
                                object_value::string(path_string),
                                self.state.string_prototype,
                            );

                            context.set_register(reg, path);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::EnvSetWorkingDirectory => {
                    let reg = instruction.arg(0);
                    let dir_ptr = context.get_register(instruction.arg(1));
                    let dir = dir_ptr.string_value()?;

                    match directories::set_working_directory(dir) {
                        Ok(_) => {
                            context.set_register(reg, dir_ptr);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self, process, error, context, index
                            );
                        }
                    }
                }
                InstructionType::EnvArguments => {
                    let reg = instruction.arg(0);
                    let args = process.allocate(
                        object_value::array(self.state.arguments.clone()),
                        self.state.array_prototype,
                    );

                    context.set_register(reg, args);
                }
                InstructionType::EnvRemove => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));

                    env::remove_var(var.string_value()?);

                    context.set_register(reg, self.state.nil_object);
                }
                InstructionType::BlockGetReceiver => {
                    let reg = instruction.arg(0);
                    let rec = context.binding.receiver;

                    context.set_register(reg, rec);
                }
                InstructionType::BlockSetReceiver => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));

                    context.binding.receiver = rec;
                    context.set_register(reg, rec);
                }
                InstructionType::RunBlockWithReceiver => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let rec_ptr = context.get_register(instruction.arg(2));
                    let block = block_ptr.block_value()?;

                    let mut new_ctx = ExecutionContext::from_block(
                        &block,
                        Some(register as u16),
                    );

                    new_ctx.binding.receiver = rec_ptr;

                    self.prepare_new_context(
                        process,
                        &instruction,
                        &new_ctx,
                        instruction.arg(3),
                        instruction.arg(4),
                        5,
                    )?;

                    process.push_context(new_ctx);

                    enter_context!(process, context, index);
                }
                InstructionType::ProcessSetPanicHandler => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));

                    process.set_panic_handler(block);
                    context.set_register(reg, block);
                }
                InstructionType::ProcessAddDeferToCaller => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));

                    if block.block_value().is_err() {
                        return Err("only Blocks can be deferred".to_string());
                    }

                    // We can not use `if let Some(...) = ...` here as the
                    // mutable borrow of "context" prevents the 2nd mutable
                    // borrow inside the "else".
                    if context.parent().is_some() {
                        context.parent_mut().unwrap().add_defer(block);
                    } else {
                        context.add_defer(block);
                    }

                    context.set_register(reg, block);
                }
                InstructionType::SetDefaultPanicHandler => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let handler =
                        self.state.set_default_panic_handler(block)?;

                    context.set_register(reg, handler);
                }
                InstructionType::ProcessPinThread => {
                    let reg = instruction.arg(0);

                    let result = if process.thread_id().is_some() {
                        self.state.false_object
                    } else {
                        process.set_thread_id(worker.thread_id);

                        self.state.true_object
                    };

                    context.set_register(reg, result);
                }
                InstructionType::ProcessUnpinThread => {
                    let reg = instruction.arg(0);

                    process.unset_thread_id();

                    context.set_register(reg, self.state.nil_object);
                }
            };
        }

        process.finished();

        write_lock!(self.state.process_table).release(process.pid);

        // We must clean up _after_ removing the process from the process table
        // to prevent a cleanup from happening while the process is still
        // receiving messages as this could lead to memory not being reclaimed.
        self.schedule_gc_for_finished_process(&process);

        // Terminate once the main process has finished execution.
        if process.is_main() {
            self.terminate();
        }

        Ok(())
    }

    /// Collects a set of arguments from an instruction.
    pub fn collect_arguments(
        &self,
        process: &RcProcess,
        instruction: &Instruction,
        offset: usize,
        amount: usize,
    ) -> Vec<ObjectPointer> {
        let mut args: Vec<ObjectPointer> = Vec::with_capacity(amount);

        for index in offset..(offset + amount) {
            let arg_index = instruction.arg(index);

            args.push(process.get_register(arg_index));
        }

        args
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    ///
    /// Returns true if a process should be suspended for garbage collection.
    fn gc_safepoint(&self, process: &RcProcess) -> bool {
        if process.should_collect_young_generation() {
            self.schedule_gc_request(GcRequest::heap(
                self.state.clone(),
                process.clone(),
            ));

            true
        } else if process.should_collect_mailbox() {
            self.schedule_gc_request(GcRequest::mailbox(
                self.state.clone(),
                process.clone(),
            ));

            true
        } else {
            false
        }
    }

    fn schedule_gc_request(&self, request: GcRequest) {
        request.process.suspend_for_gc();
        self.state.gc_pool.schedule(Job::normal(request));
    }

    fn schedule_gc_for_finished_process(&self, process: &RcProcess) {
        let request = GcRequest::finished(self.state.clone(), process.clone());
        self.state.gc_pool.schedule(Job::normal(request));
    }

    #[inline(always)]
    fn validate_number_of_arguments(
        &self,
        code: CompiledCodePointer,
        given_positional: usize,
        given_keyword: usize,
    ) -> Result<(), String> {
        let arguments = given_positional + given_keyword;

        if !code.valid_number_of_arguments(arguments) {
            return Err(format!(
                "{} takes {} arguments but {} were supplied",
                code.name.string_value().unwrap(),
                code.label_for_number_of_arguments(),
                arguments
            ));
        }

        Ok(())
    }

    fn set_positional_arguments(
        &self,
        process: &RcProcess,
        context: &ExecutionContext,
        registers: &[u16],
    ) {
        let locals = context.binding.locals_mut();

        for (index, register) in registers.iter().enumerate() {
            locals[index] = process.get_register(usize::from(*register));
        }
    }

    fn pack_excessive_arguments(
        &self,
        process: &RcProcess,
        context: &ExecutionContext,
        pack_local: usize,
        registers: &[u16],
    ) {
        let locals = context.binding.locals_mut();

        let pointers = registers
            .iter()
            .map(|register| process.get_register(usize::from(*register)))
            .collect::<Vec<ObjectPointer>>();

        locals[pack_local] = process.allocate(
            object_value::array(pointers),
            self.state.array_prototype,
        );
    }

    fn prepare_new_context(
        &self,
        process: &RcProcess,
        instruction: &Instruction,
        context: &ExecutionContext,
        given_positional: usize,
        given_keyword: usize,
        pos_start: usize,
    ) -> Result<(), String> {
        self.validate_number_of_arguments(
            context.code,
            given_positional,
            given_keyword,
        )?;

        let (excessive, pos_args) =
            context.code.number_of_arguments_to_set(given_positional);

        let pos_end = pos_start + pos_args;
        let key_start = pos_start + given_positional;

        self.set_positional_arguments(
            process,
            context,
            &instruction.arguments[pos_start..pos_end],
        );

        if excessive {
            let local_index = context.code.rest_argument_index();
            let extra = &instruction.arguments[(pos_end - 1)..key_start];

            self.pack_excessive_arguments(process, context, local_index, extra);
        }

        if given_keyword > 0 {
            self.prepare_keyword_arguments(
                process,
                instruction,
                context,
                key_start,
            );
        }

        Ok(())
    }

    fn prepare_keyword_arguments(
        &self,
        process: &RcProcess,
        instruction: &Instruction,
        context: &ExecutionContext,
        keyword_start: usize,
    ) {
        let keyword_args = &instruction.arguments[keyword_start..];
        let locals = context.binding.locals_mut();

        for slice in keyword_args.chunks(2) {
            let key = process.get_register(usize::from(slice[0]));
            let val = process.get_register(usize::from(slice[1]));

            if let Some(index) = context.code.argument_position(key) {
                locals[index] = val;
            }
        }
    }

    fn throw(
        &self,
        process: &RcProcess,
        value: ObjectPointer,
    ) -> Result<(), String> {
        let mut deferred = Vec::new();

        loop {
            let code = process.compiled_code();
            let context = process.context_mut();
            let index = context.instruction_index;

            for entry in &code.catch_table.entries {
                if entry.start < index && entry.end >= index {
                    context.instruction_index = entry.jump_to;
                    context.set_register(entry.register, value);

                    // When unwinding, move all deferred blocks to the context
                    // that handles the error. This makes unwinding easier, at
                    // the cost of making a return from this context slightly
                    // more expensive.
                    context.append_deferred_blocks(&mut deferred);

                    return Ok(());
                }
            }

            if context.parent().is_some() {
                context.move_deferred_blocks_to(&mut deferred);
            }

            if process.pop_context() {
                // Move all the pending deferred blocks from previous frames
                // into the top-level frame. These will be scheduled once we
                // return from the panic handler.
                process.context_mut().append_deferred_blocks(&mut deferred);

                return Err(format!(
                    "A thrown value reached the top-level in process {}",
                    process.pid
                ));
            }
        }
    }

    fn panic(&self, worker: &Worker, process: &RcProcess, message: &str) {
        let handler_opt = process
            .panic_handler()
            .cloned()
            .or_else(|| self.state.default_panic_handler());

        if let Some(handler) = handler_opt {
            if let Err(message) =
                self.run_custom_panic_handler(worker, process, message, handler)
            {
                self.run_default_panic_handler(process, &message);
            }
        } else {
            self.run_default_panic_handler(process, message);
        }
    }

    /// Executes a custom panic handler.
    ///
    /// Any deferred blocks will be executed before executing the registered
    /// panic handler.
    fn run_custom_panic_handler(
        &self,
        worker: &Worker,
        process: &RcProcess,
        message: &str,
        handler: ObjectPointer,
    ) -> Result<(), String> {
        let block = handler.block_value()?;

        self.validate_number_of_arguments(block.code, 1, 0)?;

        let mut new_context = ExecutionContext::from_block(block, None);

        let error = process.allocate(
            object_value::string(message.to_string()),
            self.state.string_prototype,
        );

        new_context.terminate_upon_return();
        new_context.binding.locals_mut()[0] = error;

        process.push_context(new_context);

        // We want to schedule any remaining deferred blocks _before_ running
        // the panic handler. This way, if the panic handler hard terminates, we
        // still run the deferred blocks.
        process
            .context_mut()
            .schedule_deferred_blocks_of_all_parents(process)?;

        self.run_with_error_handling(worker, &process);

        Ok(())
    }

    /// Executes the default panic handler.
    ///
    /// This handler will _not_ execute any deferred blocks.
    fn run_default_panic_handler(&self, process: &RcProcess, message: &str) {
        runtime_panic::display_panic(process, message);

        self.terminate_for_panic();
    }

    fn terminate_for_panic(&self) {
        self.state.set_exit_status(1);
        self.terminate();
    }

    fn unwind_until_defining_scope(&self, process: &RcProcess) {
        let top_binding = process.context().top_binding_pointer();

        loop {
            let context = process.context();

            if context.binding_pointer() == top_binding {
                return;
            } else {
                process.pop_context();
            }
        }
    }
}
