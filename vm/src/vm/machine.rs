//! Virtual Machine for running instructions
use crate::compiled_code::CompiledCodePointer;
use crate::execution_context::ExecutionContext;
use crate::gc::request::Request as GcRequest;
use crate::integer_operations;
use crate::module_registry::{ModuleRegistry, RcModuleRegistry};
use crate::network_poller::worker::Worker as NetworkPollerWorker;
use crate::numeric::division::{FlooredDiv, OverflowingFlooredDiv};
use crate::numeric::modulo::{Modulo, OverflowingModulo};
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::runtime_panic;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::pool::Pool;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::array;
use crate::vm::block;
use crate::vm::byte_array;
use crate::vm::env;
use crate::vm::ffi;
use crate::vm::float;
use crate::vm::hasher;
use crate::vm::instruction::{Instruction, InstructionType};
use crate::vm::integer;
use crate::vm::io;
use crate::vm::module;
use crate::vm::object;
use crate::vm::process;
use crate::vm::socket;
use crate::vm::state::RcState;
use crate::vm::string;
use crate::vm::time;
use num_bigint::BigInt;
use rayon::ThreadPoolBuilder;
use std::i32;
use std::ops::{Add, Mul, Sub};
use std::panic;
use std::thread;

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

macro_rules! enter_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context.instruction_index = $index;

        reset_context!($process, $context, $index);
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
            $vm.state.scheduler.schedule($process.clone());
            return Ok(());
        }
    }};
}

macro_rules! try_runtime_error {
    ($expr:expr, $vm:expr, $proc:expr, $context:ident, $index:ident) => {{
        // When an operation would block, the socket is already registered, and
        // the process may already be running again in another thread. This
        // means that when a WouldBlock is produced it is not safe to access any
        // process data.
        //
        // To ensure blocking operations are retried properly, we _first_ set
        // the instruction index, then advance it again if it is safe to do so.
        $context.instruction_index = $index - 1;

        match $expr {
            Ok(thing) => {
                $context.instruction_index = $index;

                thing
            }
            Err(RuntimeError::Panic(msg)) => {
                $context.instruction_index = $index;

                return Err(msg);
            }
            Err(RuntimeError::Exception(msg)) => {
                throw_error_message!($vm, $proc, msg, $context, $index);
                continue;
            }
            Err(RuntimeError::WouldBlock) => {
                // *DO NOT* use "$context" at this point, as it may have been
                // invalidated if the process is already running again in
                // another thread.
                return Ok(());
            }
        }
    }};
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
        self.schedule_main_process(file);

        let gc_pool_guard = self.start_gc_threads();
        let finalizer_pool_guard = self.start_finalizer_threads();
        let secondary_guard = self.start_blocking_threads();
        let timeout_guard = self.start_timeout_worker_thread();

        // The network poller doesn't produce a guard, because there's no
        // cross-platform way of waking up the system poller, so we just don't
        // wait for it to finish when terminating.
        self.start_network_poller_thread();

        // Starting the primary threads will block this thread, as the main
        // worker will run directly onto the current thread. As such, we must
        // start these threads last.
        let primary_guard = self.start_primary_threads();

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output.
        if primary_guard.join().is_err()
            || secondary_guard.join().is_err()
            || gc_pool_guard.join().is_err()
            || finalizer_pool_guard.join().is_err()
            || timeout_guard.join().is_err()
        {
            self.state.set_exit_status(1);
        }
    }

    fn configure_rayon(&self) {
        ThreadPoolBuilder::new()
            .thread_name(|idx| format!("rayon {}", idx))
            .num_threads(self.state.config.generic_parallel_threads)
            .build_global()
            .unwrap();
    }

    fn start_primary_threads(&self) -> JoinList<()> {
        let machine = self.clone();

        self.state
            .scheduler
            .primary_pool
            .start_main(move |worker, process| {
                machine.run_with_error_handling(worker, &process)
            })
    }

    fn start_blocking_threads(&self) -> JoinList<()> {
        let machine = self.clone();

        self.state
            .scheduler
            .blocking_pool
            .start(move |worker, process| {
                machine.run_with_error_handling(worker, &process)
            })
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> JoinList<()> {
        self.state
            .gc_pool
            .start(move |_, mut request| request.perform())
    }

    fn start_finalizer_threads(&self) -> JoinList<()> {
        self.state.finalizer_pool.start(move |_, blocks| {
            for mut block in blocks {
                block.finalize_pending();
            }
        })
    }

    fn start_timeout_worker_thread(&self) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name("timeout worker".to_string())
            .spawn(move || {
                state.timeout_worker.run(&state.scheduler);
            })
            .unwrap()
    }

    fn start_network_poller_thread(&self) {
        let state = self.state.clone();

        thread::Builder::new()
            .name("network poller".to_string())
            .spawn(move || {
                NetworkPollerWorker::new(state).run();
            })
            .unwrap();
    }

    fn terminate(&self) {
        self.state.scheduler.terminate();
        self.state.gc_pool.terminate();
        self.state.finalizer_pool.terminate();
        self.state.timeout_worker.terminate();
    }

    pub fn schedule_main_process(&self, file: &str) {
        let process = {
            let (block, _) =
                module::load_string(&self.state, &self.module_registry, file)
                    .unwrap();

            process::allocate(&self.state, &block)
        };

        process.set_main();

        self.state.scheduler.schedule_on_main_thread(process);
    }

    /// Executes a single process, terminating in the event of an error.
    pub fn run_with_error_handling(
        &self,
        worker: &mut ProcessWorker,
        process: &RcProcess,
    ) {
        // We are using AssertUnwindSafe here so we can pass a &mut Worker to
        // run()/panic(). This might be risky if values captured are not unwind
        // safe, so take care when capturing new variables.
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            if let Err(message) = self.run(worker, process) {
                self.panic(worker, process, &message);
            }
        }));

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
    #[cfg_attr(
        feature = "cargo-clippy",
        allow(cyclomatic_complexity, cognitive_complexity)
    )]
    pub fn run(
        &self,
        worker: &mut ProcessWorker,
        process: &RcProcess,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions;

        let mut context;
        let mut index;
        let mut instruction;

        reset_context!(process, context, index);

        'exec_loop: loop {
            instruction = unsafe { context.code.instruction(index) };
            index += 1;

            match instruction.instruction_type {
                InstructionType::SetLiteral => {
                    let reg = instruction.arg(0);
                    let index = instruction.arg(1);
                    let literal = unsafe { context.code.literal(index) };

                    context.set_register(reg, literal);
                }
                InstructionType::SetObject => {
                    let register = instruction.arg(0);
                    let perm = context.get_register(instruction.arg(1));
                    let proto =
                        instruction.arg_opt(2).map(|r| context.get_register(r));

                    let obj = object::create(&self.state, process, perm, proto);

                    context.set_register(register, obj);
                }
                InstructionType::SetArray => {
                    let register = instruction.arg(0);
                    let val_count = instruction.arguments.len() - 1;
                    let obj = array::create(
                        &self.state,
                        process,
                        &instruction.arguments[1..=val_count],
                    );

                    context.set_register(register, obj);
                }
                InstructionType::GetBuiltinPrototype => {
                    let reg = instruction.arg(0);
                    let id = context.get_register(instruction.arg(1));
                    let proto =
                        object::prototype_for_identifier(&self.state, id)?;

                    context.set_register(reg, proto);
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
                    let obj = block::create(
                        &self.state,
                        process,
                        cc,
                        instruction.arg_opt(2).map(|r| context.get_register(r)),
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

                    let object = instruction
                        .arg_opt(1)
                        .map(|r| context.get_register(r))
                        .unwrap_or(self.state.nil_object);

                    if block_return {
                        process::unwind_until_defining_scope(process);

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

                    if is_false!(self.state, context.get_register(value_reg)) {
                        index = instruction.arg(0);
                    }
                }
                InstructionType::GotoIfTrue => {
                    let value_reg = instruction.arg(1);

                    if !is_false!(self.state, context.get_register(value_reg)) {
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
                        return Err(
                            "Can not divide an Integer by 0".to_string()
                        );
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
                    let integer = context.get_register(instruction.arg(1));
                    let obj = integer::to_float(&self.state, process, integer)?;

                    context.set_register(register, obj);
                }
                InstructionType::IntegerToString => {
                    let register = instruction.arg(0);
                    let integer = context.get_register(instruction.arg(1));
                    let obj =
                        integer::to_string(&self.state, process, integer)?;

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
                    let reg = instruction.arg(0);
                    let float = context.get_register(instruction.arg(1));
                    let obj = float::to_integer(&self.state, process, float)?;

                    context.set_register(reg, obj);
                }
                InstructionType::FloatToString => {
                    let reg = instruction.arg(0);
                    let float = context.get_register(instruction.arg(1));
                    let obj = float::to_string(&self.state, process, float)?;

                    context.set_register(reg, obj);
                }
                InstructionType::FloatSmaller => {
                    float_bool_op!(self.state, context, instruction, <);
                }
                InstructionType::FloatGreater => {
                    float_bool_op!(self.state, context, instruction, >);
                }
                InstructionType::FloatEquals => {
                    let reg = instruction.arg(0);
                    let compare = context.get_register(instruction.arg(1));
                    let compare_with = context.get_register(instruction.arg(2));
                    let obj = float::equal(&self.state, compare, compare_with)?;

                    context.set_register(reg, obj);
                }
                InstructionType::FloatGreaterOrEqual => {
                    float_bool_op!(self.state, context, instruction, >=);
                }
                InstructionType::FloatSmallerOrEqual => {
                    float_bool_op!(self.state, context, instruction, <=);
                }
                InstructionType::ArraySet => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let in_value = context.get_register(instruction.arg(3));
                    let out_value = array::set(
                        &self.state,
                        process,
                        array,
                        index,
                        in_value,
                    )?;

                    context.set_register(reg, out_value);
                }
                InstructionType::ArrayAt => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let value = array::get(&self.state, array, index)?;

                    context.set_register(reg, value);
                }
                InstructionType::ArrayRemove => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let value = array::remove(&self.state, array, index)?;

                    context.set_register(reg, value);
                }
                InstructionType::ArrayLength => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let length = array::length(&self.state, process, array)?;

                    context.set_register(reg, length);
                }
                InstructionType::ArrayClear => {
                    let array = context.get_register(instruction.arg(0));

                    array::clear(array)?;
                }
                InstructionType::StringToLower => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let obj = string::to_lower(&self.state, process, string)?;

                    context.set_register(reg, obj);
                }
                InstructionType::StringToUpper => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let obj = string::to_upper(&self.state, process, string)?;

                    context.set_register(reg, obj);
                }
                InstructionType::StringEquals => {
                    let reg = instruction.arg(0);
                    let comp = context.get_register(instruction.arg(1));
                    let comp_with = context.get_register(instruction.arg(2));
                    let obj = string::equal(&self.state, comp, comp_with)?;

                    context.set_register(reg, obj);
                }
                InstructionType::StringToByteArray => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let obj =
                        string::to_byte_array(&self.state, process, string)?;

                    context.set_register(reg, obj);
                }
                InstructionType::StringLength => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let length = string::length(&self.state, process, string)?;

                    context.set_register(reg, length);
                }
                InstructionType::StringSize => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let size = string::byte_size(&self.state, process, string)?;

                    context.set_register(reg, size);
                }
                InstructionType::StdoutWrite => {
                    let reg = instruction.arg(0);
                    let input = context.get_register(instruction.arg(1));
                    let size = try_runtime_error!(
                        io::stdout_write(&self.state, process, input),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::StderrWrite => {
                    let reg = instruction.arg(0);
                    let input = context.get_register(instruction.arg(1));
                    let size = try_runtime_error!(
                        io::stderr_write(&self.state, process, input),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::StdoutFlush => {
                    let reg = instruction.arg(0);
                    let obj = try_runtime_error!(
                        io::stdout_flush(&self.state),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::StderrFlush => {
                    let reg = instruction.arg(0);
                    let obj = try_runtime_error!(
                        io::stderr_flush(&self.state),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::StdinRead => {
                    let reg = instruction.arg(0);
                    let buff = context.get_register(instruction.arg(1));
                    let max = context.get_register(instruction.arg(2));
                    let obj = try_runtime_error!(
                        io::stdin_read(&self.state, process, buff, max),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::FileOpen => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let mode = context.get_register(instruction.arg(2));
                    let file = try_runtime_error!(
                        io::open_file(&self.state, process, path, mode),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, file);
                }
                InstructionType::FileWrite => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let input = context.get_register(instruction.arg(2));
                    let size = try_runtime_error!(
                        io::write_file(&self.state, process, file, input),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::FileRead => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let max = context.get_register(instruction.arg(3));
                    let obj = try_runtime_error!(
                        io::read_file(&self.state, process, file, buff, max),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::FileFlush => {
                    let file = context.get_register(instruction.arg(0));

                    try_runtime_error!(
                        io::flush_file(&self.state, file),
                        self,
                        process,
                        context,
                        index
                    );
                }
                InstructionType::FileSize => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let size = try_runtime_error!(
                        io::file_size(&self.state, process, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::FileSeek => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let offset = context.get_register(instruction.arg(2));
                    let cursor = try_runtime_error!(
                        io::seek_file(&self.state, process, file, offset),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, cursor);
                }
                InstructionType::LoadModule => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));

                    let (block, execute) = {
                        module::load(&self.state, &self.module_registry, path)?
                    };

                    if execute {
                        let new_context = ExecutionContext::from_block(
                            &block,
                            Some(reg as u16),
                        );

                        process.push_context(new_context);

                        enter_context!(process, context, index);
                    } else {
                        context.set_register(reg, self.state.nil_object);
                    }
                }
                InstructionType::SetAttribute => {
                    let reg = instruction.arg(0);
                    let target = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let value = context.get_register(instruction.arg(3));

                    let obj = object::set_attribute(
                        &self.state,
                        process,
                        target,
                        name,
                        value,
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::SetAttributeToObject => {
                    let reg = instruction.arg(0);
                    let obj = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));

                    let attr = object::set_attribute_to_object(
                        &self.state,
                        process,
                        obj,
                        name,
                    );

                    context.set_register(reg, attr);
                }
                InstructionType::GetAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let attr = object::get_attribute(&self.state, rec, name);

                    context.set_register(reg, attr);
                }
                InstructionType::SetPrototype => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let proto = context.get_register(instruction.arg(2));
                    let obj =
                        object::set_prototype(&self.state, process, src, proto);

                    context.set_register(reg, obj);
                }
                InstructionType::GetPrototype => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let proto = object::get_prototype(&self.state, src);

                    context.set_register(reg, proto);
                }
                InstructionType::LocalExists => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = process::local_exists(&self.state, process, idx);

                    context.set_register(reg, res);
                }
                InstructionType::ProcessSpawn => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let proc = process::spawn(&self.state, process, block)?;

                    context.set_register(reg, proc);
                }
                InstructionType::ProcessSendMessage => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let msg = context.get_register(instruction.arg(2));
                    let res =
                        process::send_message(&self.state, process, rec, msg)?;

                    context.set_register(reg, res);
                }
                InstructionType::ProcessReceiveMessage => {
                    let reg = instruction.arg(0);
                    let time_ptr = context.get_register(instruction.arg(1));

                    if let Some(message) =
                        process::receive_message(&self.state, process)
                    {
                        context.set_register(reg, message);
                        continue;
                    }

                    // We *must* save the instruction index first. If we save
                    // this later on, a copy of this process scheduled by
                    // another thread (because it sent the process a message)
                    // may end up running the wrong instructions and/or corrupt
                    // registers in the process.
                    context.instruction_index = index - 1;

                    process::wait_for_message(
                        &self.state,
                        process,
                        process::optional_timeout(time_ptr)?,
                    );

                    return Ok(());
                }
                InstructionType::ProcessCurrent => {
                    let reg = instruction.arg(0);
                    let obj = process.allocate(
                        object_value::process(process.clone()),
                        self.state.process_prototype,
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::ProcessSuspendCurrent => {
                    let time_ptr = context.get_register(instruction.arg(0));
                    let timeout = process::optional_timeout(time_ptr)?;

                    context.instruction_index = index;

                    process::suspend(&self.state, process, timeout);

                    return Ok(());
                }
                InstructionType::SetParentLocal => {
                    let local = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let value = context.get_register(instruction.arg(2));

                    process::set_parent_local(process, local, depth, value)?;
                }
                InstructionType::GetParentLocal => {
                    let reg = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let local = instruction.arg(2);
                    let val = process::get_parent_local(process, local, depth)?;

                    context.set_register(reg, val)
                }
                InstructionType::ObjectEquals => {
                    let reg = instruction.arg(0);
                    let comp = context.get_register(instruction.arg(1));
                    let comp_with = context.get_register(instruction.arg(2));
                    let res = object::equal(&self.state, comp, comp_with);

                    context.set_register(reg, res);
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
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::attribute_exists(&self.state, src, name);

                    context.set_register(reg, res);
                }
                InstructionType::RemoveAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::remove_attribute(&self.state, rec, name);

                    context.set_register(reg, res);
                }
                InstructionType::GetAttributeNames => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let res =
                        object::attribute_names(&self.state, process, rec);

                    context.set_register(reg, res);
                }
                InstructionType::TimeMonotonic => {
                    let reg = instruction.arg(0);
                    let res = time::monotonic(&self.state, process);

                    context.set_register(reg, res);
                }
                InstructionType::RunBlock => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let block = block_ptr.block_value()?;

                    let mut new_ctx = ExecutionContext::from_block(
                        &block,
                        Some(register as u16),
                    );

                    self.prepare_new_context(
                        process,
                        &instruction,
                        &mut new_ctx,
                        instruction.arg(2),
                        instruction.arg(3),
                        4,
                    )?;

                    process.push_context(new_ctx);

                    enter_context!(process, context, index);
                }
                InstructionType::SetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let val = context.get_register(instruction.arg(2));
                    let res =
                        process::set_global(&self.state, process, idx, val);

                    context.set_register(reg, res);
                }
                InstructionType::GetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let val = process.get_global(idx);

                    context.set_register(reg, val);
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
                    let target = context.get_register(instruction.arg(0));
                    let source = context.get_register(instruction.arg(1));

                    object::copy_blocks(&self.state, target, source);
                }
                InstructionType::FloatIsNan => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::is_nan(&self.state, ptr);

                    context.set_register(reg, res);
                }
                InstructionType::FloatIsInfinite => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::is_infinite(&self.state, ptr);

                    context.set_register(reg, res);
                }
                InstructionType::FloatFloor => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::floor(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                InstructionType::FloatCeil => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::ceil(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                InstructionType::FloatRound => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let prec = context.get_register(instruction.arg(2));
                    let res = float::round(&self.state, process, ptr, prec)?;

                    context.set_register(reg, res);
                }
                InstructionType::Drop => {
                    let ptr = context.get_register(instruction.arg(0));

                    object::drop_value(ptr);
                }
                InstructionType::ProcessSetBlocking => {
                    let reg = instruction.arg(0);
                    let blocking_ptr = context.get_register(instruction.arg(1));
                    let is_blocking = blocking_ptr == self.state.true_object;

                    if process.is_pinned() {
                        // If a process is pinned we can't move it to another
                        // pool. We can't panic in this case, since it would
                        // prevent code from using certain IO operations that
                        // may try to move the process to another pool.
                        //
                        // Instead, we simply ignore the request and continue
                        // running on the current thread.
                        context.set_register(reg, self.state.false_object);

                        continue;
                    }

                    if is_blocking == process.is_blocking() {
                        context.set_register(reg, self.state.false_object);
                    } else {
                        process.set_blocking(is_blocking);
                        context.set_register(reg, self.state.true_object);
                        context.instruction_index = index;

                        // After this we can _not_ perform any operations on the
                        // process any more as it might be concurrently modified
                        // by the pool we just moved it to.
                        self.state.scheduler.schedule(process.clone());

                        return Ok(());
                    }
                }
                InstructionType::FileRemove => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let obj = try_runtime_error!(
                        io::remove_file(&self.state, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::Panic => {
                    let msg = context.get_register(instruction.arg(0));

                    context.line = instruction.line;

                    return Err(msg.string_value()?.to_owned_string());
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
                    let reg = instruction.arg(0);
                    let res = env::operating_system(&self.state);

                    context.set_register(reg, res);
                }
                InstructionType::FileCopy => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let dst = context.get_register(instruction.arg(2));
                    let obj = try_runtime_error!(
                        io::copy_file(&self.state, process, src, dst),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, obj);
                }
                InstructionType::FileType => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::file_type(path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::FileTime => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let time = try_runtime_error!(
                        io::file_time(&self.state, process, path, kind),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, time);
                }
                InstructionType::TimeSystem => {
                    let reg = instruction.arg(0);
                    let res = time::system(&self.state, process);

                    context.set_register(reg, res);
                }
                InstructionType::TimeSystemOffset => {
                    let reg = instruction.arg(0);
                    let res = time::system_offset();

                    context.set_register(reg, res);
                }
                InstructionType::TimeSystemDst => {
                    let reg = instruction.arg(0);
                    let res = time::system_dst(&self.state);

                    context.set_register(reg, res);
                }
                InstructionType::DirectoryCreate => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let recursive = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::create_directory(&self.state, path, recursive),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::DirectoryRemove => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let recursive = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::remove_directory(&self.state, path, recursive),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::DirectoryList => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let array = try_runtime_error!(
                        io::list_directory(&self.state, process, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, array);
                }
                InstructionType::StringConcat => {
                    let reg = instruction.arg(0);
                    let left = context.get_register(instruction.arg(1));
                    let right = context.get_register(instruction.arg(2));
                    let res =
                        string::concat(&self.state, process, left, right)?;

                    context.set_register(reg, res);
                }
                InstructionType::HasherNew => {
                    let reg = instruction.arg(0);
                    let res = hasher::create(&self.state, process);

                    context.set_register(reg, res);
                }
                InstructionType::HasherWrite => {
                    let reg = instruction.arg(0);
                    let hasher = context.get_register(instruction.arg(1));
                    let value = context.get_register(instruction.arg(2));
                    let res = hasher::write(&self.state, hasher, value)?;

                    context.set_register(reg, res);
                }
                InstructionType::HasherFinish => {
                    let reg = instruction.arg(0);
                    let hasher = context.get_register(instruction.arg(1));
                    let res = hasher::finish(&self.state, process, hasher)?;

                    context.set_register(reg, res);
                }
                InstructionType::Stacktrace => {
                    let reg = instruction.arg(0);
                    let limit = context.get_register(instruction.arg(1));
                    let skip = context.get_register(instruction.arg(2));
                    let res =
                        process::stacktrace(&self.state, process, limit, skip)?;

                    context.set_register(reg, res);
                }
                InstructionType::ProcessTerminateCurrent => {
                    break 'exec_loop;
                }
                InstructionType::StringSlice => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let start = context.get_register(instruction.arg(2));
                    let amount = context.get_register(instruction.arg(3));
                    let res = string::slice(
                        &self.state,
                        process,
                        string,
                        start,
                        amount,
                    )?;

                    context.set_register(reg, res);
                }
                InstructionType::BlockMetadata => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let field = context.get_register(instruction.arg(2));
                    let res =
                        block::metadata(&self.state, process, block, field)?;

                    context.set_register(reg, res);
                }
                InstructionType::StringFormatDebug => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let res =
                        string::format_debug(&self.state, process, string)?;

                    context.set_register(reg, res);
                }
                InstructionType::StringConcatMultiple => {
                    let reg = instruction.arg(0);
                    let strings = context.get_register(instruction.arg(1));
                    let res =
                        string::concat_multiple(&self.state, process, strings)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayFromArray => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let res = byte_array::create(&self.state, process, array)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArraySet => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = byte_array::set(array, index, val)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayAt => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let res = byte_array::get(&self.state, array, index)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayRemove => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let index = context.get_register(instruction.arg(2));
                    let res = byte_array::remove(&self.state, array, index)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayLength => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let res = byte_array::length(&self.state, process, array)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayClear => {
                    let array = context.get_register(instruction.arg(0));

                    byte_array::clear(array)?;
                }
                InstructionType::ByteArrayEquals => {
                    let reg = instruction.arg(0);
                    let compare = context.get_register(instruction.arg(1));
                    let compare_with = context.get_register(instruction.arg(2));
                    let res =
                        byte_array::equals(&self.state, compare, compare_with)?;

                    context.set_register(reg, res);
                }
                InstructionType::ByteArrayToString => {
                    let reg = instruction.arg(0);
                    let array = context.get_register(instruction.arg(1));
                    let drain = context.get_register(instruction.arg(2));
                    let res = byte_array::to_string(
                        &self.state,
                        process,
                        array,
                        drain,
                    )?;

                    context.set_register(reg, res);
                }
                InstructionType::EnvGet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = env::get(&self.state, process, var)?;

                    context.set_register(reg, val);
                }
                InstructionType::EnvSet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = context.get_register(instruction.arg(2));

                    context.set_register(reg, env::set(var, val)?);
                }
                InstructionType::EnvVariables => {
                    let reg = instruction.arg(0);
                    let names = env::names(&self.state, process)?;

                    context.set_register(reg, names);
                }
                InstructionType::EnvHomeDirectory => {
                    let reg = instruction.arg(0);
                    let path = env::home_directory(&self.state, process)?;

                    context.set_register(reg, path);
                }
                InstructionType::EnvTempDirectory => {
                    let reg = instruction.arg(0);
                    let path = env::tmp_directory(&self.state, process);

                    context.set_register(reg, path);
                }
                InstructionType::EnvGetWorkingDirectory => {
                    let reg = instruction.arg(0);
                    let path = try_runtime_error!(
                        env::working_directory(&self.state, process),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, path);
                }
                InstructionType::EnvSetWorkingDirectory => {
                    let reg = instruction.arg(0);
                    let dir = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        env::set_working_directory(dir),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::EnvArguments => {
                    let reg = instruction.arg(0);
                    let args = env::arguments(&self.state, process);

                    context.set_register(reg, args);
                }
                InstructionType::EnvRemove => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = env::remove(&self.state, var)?;

                    context.set_register(reg, val);
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
                        &mut new_ctx,
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
                    let res = process::add_defer_to_caller(process, block)?;

                    context.set_register(reg, res);
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
                    let res = process::pin_thread(&self.state, process, worker);

                    context.set_register(reg, res);
                }
                InstructionType::ProcessUnpinThread => {
                    let reg = instruction.arg(0);
                    let res =
                        process::unpin_thread(&self.state, process, worker);

                    context.set_register(reg, res);
                }
                InstructionType::ProcessIdentifier => {
                    let reg = instruction.arg(0);
                    let proc = context.get_register(instruction.arg(1));
                    let res = process::identifier(&self.state, process, proc)?;

                    context.set_register(reg, res);
                }
                InstructionType::LibraryOpen => {
                    let reg = instruction.arg(0);
                    let names = context.get_register(instruction.arg(1));
                    let res = ffi::open_library(&self.state, process, names)?;

                    context.set_register(reg, res);
                }
                InstructionType::FunctionAttach => {
                    let reg = instruction.arg(0);
                    let lib = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let arg_types = context.get_register(instruction.arg(3));
                    let rtype = context.get_register(instruction.arg(4));
                    let res = ffi::attach_function(
                        &self.state,
                        process,
                        lib,
                        name,
                        arg_types,
                        rtype,
                    )?;

                    context.set_register(reg, res);
                }
                InstructionType::FunctionCall => {
                    let reg = instruction.arg(0);
                    let func = context.get_register(instruction.arg(1));
                    let args = context.get_register(instruction.arg(2));
                    let res =
                        ffi::call_function(&self.state, process, func, args)?;

                    context.set_register(reg, res);
                }
                InstructionType::PointerAttach => {
                    let reg = instruction.arg(0);
                    let lib = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res =
                        ffi::attach_pointer(&self.state, process, lib, name)?;

                    context.set_register(reg, res);
                }
                InstructionType::PointerRead => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let read_as = context.get_register(instruction.arg(2));
                    let offset = context.get_register(instruction.arg(3));
                    let res = ffi::read_pointer(
                        &self.state,
                        process,
                        ptr,
                        read_as,
                        offset,
                    )?;

                    context.set_register(reg, res);
                }
                InstructionType::PointerWrite => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let write_as = context.get_register(instruction.arg(2));
                    let value = context.get_register(instruction.arg(3));
                    let offset = context.get_register(instruction.arg(4));
                    let res = ffi::write_pointer(ptr, write_as, value, offset)?;

                    context.set_register(reg, res);
                }
                InstructionType::PointerFromAddress => {
                    let reg = instruction.arg(0);
                    let addr = context.get_register(instruction.arg(1));
                    let res =
                        ffi::pointer_from_address(&self.state, process, addr)?;

                    context.set_register(reg, res);
                }
                InstructionType::PointerAddress => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = ffi::pointer_address(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                InstructionType::ForeignTypeSize => {
                    let reg = instruction.arg(0);
                    let kind = context.get_register(instruction.arg(1));
                    let res = ffi::type_size(kind)?;

                    context.set_register(reg, res);
                }
                InstructionType::ForeignTypeAlignment => {
                    let reg = instruction.arg(0);
                    let kind = context.get_register(instruction.arg(1));
                    let res = ffi::type_alignment(kind)?;

                    context.set_register(reg, res);
                }
                InstructionType::StringToInteger => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let rdx = context.get_register(instruction.arg(2));
                    let value = try_runtime_error!(
                        string::to_integer(&self.state, process, val, rdx),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, value);
                }
                InstructionType::StringToFloat => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let value = try_runtime_error!(
                        string::to_float(&self.state, process, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, value);
                }
                InstructionType::FloatToBits => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = float::to_bits(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                InstructionType::SocketCreate => {
                    let reg = instruction.arg(0);
                    let domain = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::create(&self.state, process, domain, kind),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketWrite => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let input = context.get_register(instruction.arg(2));
                    let size = try_runtime_error!(
                        socket::write(&self.state, process, sock, input),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::SocketRead => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let amount = context.get_register(instruction.arg(3));
                    let size = try_runtime_error!(
                        socket::read(&self.state, process, sock, buff, amount),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, size);
                }
                InstructionType::SocketAccept => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        socket::accept(&self.state, process, sock),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketReceiveFrom => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let amount = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::receive_from(
                            &self.state,
                            process,
                            sock,
                            buff,
                            amount
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketSendTo => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let addr = context.get_register(instruction.arg(3));
                    let port = context.get_register(instruction.arg(4));
                    let res = try_runtime_error!(
                        socket::send_to(
                            &self.state,
                            process,
                            sock,
                            buff,
                            addr,
                            port
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketAddress => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::address(&self.state, process, sock, kind),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketGetOption => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let opt = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::get_option(&self.state, process, sock, opt),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketSetOption => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let opt = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::set_option(&self.state, sock, opt, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketBind => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let addr = context.get_register(instruction.arg(2));
                    let port = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::bind(&self.state, process, sock, addr, port),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketListen => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let backlog = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::listen(sock, backlog),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketConnect => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let addr = context.get_register(instruction.arg(2));
                    let port = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::connect(&self.state, process, sock, addr, port),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                InstructionType::SocketShutdown => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let mode = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::shutdown(&self.state, sock, mode),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
            };
        }

        if process.is_pinned() {
            // A pinned process can only run on the corresponding worker.
            // Because pinned workers won't run already unpinned processes, and
            // because processes can't be pinned until they run, this means
            // there will only ever be one process that triggers this code.
            worker.leave_exclusive_mode();
        }

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
        self.state.gc_pool.schedule(request);
    }

    fn schedule_gc_for_finished_process(&self, process: &RcProcess) {
        let request = GcRequest::finished(self.state.clone(), process.clone());

        self.schedule_gc_request(request);
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
        context: &mut ExecutionContext,
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
        context: &mut ExecutionContext,
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
        context: &mut ExecutionContext,
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
            let extra = &instruction.arguments[pos_end..key_start];

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
        context: &mut ExecutionContext,
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
                    "A thrown value reached the top-level in process {:#x}",
                    process.identifier()
                ));
            }
        }
    }

    fn panic(
        &self,
        worker: &mut ProcessWorker,
        process: &RcProcess,
        message: &str,
    ) {
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
        worker: &mut ProcessWorker,
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
}
