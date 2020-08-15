//! Virtual Machine for running instructions
use crate::execution_context::ExecutionContext;
use crate::gc::collection::Collection;
use crate::integer_operations;
use crate::network_poller::worker::Worker as NetworkPollerWorker;
use crate::numeric::division::{FlooredDiv, OverflowingFlooredDiv};
use crate::numeric::modulo::{Modulo, OverflowingModulo};
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::runtime_panic;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::instruction::Opcode;
use crate::vm::instructions::array;
use crate::vm::instructions::block;
use crate::vm::instructions::byte_array;
use crate::vm::instructions::env;
use crate::vm::instructions::ffi;
use crate::vm::instructions::float;
use crate::vm::instructions::general;
use crate::vm::instructions::hasher;
use crate::vm::instructions::integer;
use crate::vm::instructions::io;
use crate::vm::instructions::module;
use crate::vm::instructions::object;
use crate::vm::instructions::process;
use crate::vm::instructions::random;
use crate::vm::instructions::socket;
use crate::vm::instructions::string;
use crate::vm::instructions::time;
use crate::vm::state::RcState;
use num_bigint::BigInt;
use std::i32;
use std::ops::{Add, Mul, Sub};
use std::panic;
use std::thread;

/// The name of the module that acts as the entry point in an Inko program.
const MAIN_MODULE_NAME: &str = "main";

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
    /// The shared virtual machine state, such as the process pools and built-in
    /// types.
    pub state: RcState,
}

impl Machine {
    pub fn new(state: RcState) -> Self {
        Machine { state }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until the program finishes.
    pub fn start(&self, path: &str) {
        self.parse_image(path);
        self.schedule_main_process(MAIN_MODULE_NAME);

        let gc_pool_guard = self.start_gc_threads();
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
            || timeout_guard.join().is_err()
        {
            self.state.set_exit_status(1);
        }
    }

    fn start_primary_threads(&self) -> JoinList<()> {
        self.state.scheduler.primary_pool.start_main(self.clone())
    }

    fn start_blocking_threads(&self) -> JoinList<()> {
        self.state.scheduler.blocking_pool.start(self.clone())
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> JoinList<()> {
        self.state.gc_pool.start(self.state.clone())
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

    fn parse_image(&self, path: &str) {
        self.state.parse_image(path).unwrap();
    }

    fn schedule_main_process(&self, name: &str) {
        let process = {
            let (_, block, _) =
                module::module_load_string(&self.state, name).unwrap();

            process::process_allocate(&self.state, &block)
        };

        process.set_main();

        self.state.scheduler.schedule_on_main_thread(process);
    }

    /// Executes a single process, terminating in the event of an error.
    pub fn run_with_error_handling(
        &mut self,
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
        &mut self,
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

            match instruction.opcode {
                Opcode::SetLiteral => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::set_literal(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::SetLiteralWide => {
                    let reg = instruction.arg(0);
                    let arg1 = instruction.arg(1);
                    let arg2 = instruction.arg(2);
                    let res = general::set_literal_wide(context, arg1, arg2);

                    context.set_register(reg, res);
                }
                Opcode::Allocate => {
                    let reg = instruction.arg(0);
                    let perm = context.get_register(instruction.arg(1));
                    let proto = context.get_register(instruction.arg(2));
                    let res =
                        object::allocate(&self.state, process, perm, proto);

                    context.set_register(reg, res);
                }
                Opcode::SetArray => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = array::set_array(
                        &self.state,
                        process,
                        context,
                        start,
                        len,
                    );

                    context.set_register(reg, res);
                }
                Opcode::GetBuiltinPrototype => {
                    let reg = instruction.arg(0);
                    let id = context.get_register(instruction.arg(1));
                    let proto = object::get_builtin_prototype(&self.state, id)?;

                    context.set_register(reg, proto);
                }
                Opcode::GetTrue => {
                    let res = self.state.true_object;

                    context.set_register(instruction.arg(0), res);
                }
                Opcode::GetFalse => {
                    let res = self.state.false_object;

                    context.set_register(instruction.arg(0), res);
                }
                Opcode::SetLocal => {
                    let idx = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));

                    general::set_local(context, idx, val);
                }
                Opcode::GetLocal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_local(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::SetBlock => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let rec = context.get_register(instruction.arg(2));
                    let res = block::set_block(
                        &self.state,
                        process,
                        context,
                        idx,
                        rec,
                    );

                    context.set_register(reg, res);
                }
                Opcode::Return => {
                    // If there are any pending deferred blocks, execute these
                    // first, then retry this instruction.
                    if context.schedule_deferred_blocks(process)? {
                        remember_and_reset!(process, context, index);
                    }

                    if context.terminate_upon_return {
                        break 'exec_loop;
                    }

                    let method_return = instruction.arg(0) == 1;
                    let res = context.get_register(instruction.arg(1));

                    process.set_result(res);

                    if method_return {
                        process::process_unwind_until_defining_scope(process);
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
                Opcode::GotoIfFalse => {
                    let val = context.get_register(instruction.arg(1));

                    if is_false!(self.state, val) {
                        index = instruction.arg(0) as usize;
                    }
                }
                Opcode::GotoIfTrue => {
                    let val = context.get_register(instruction.arg(1));

                    if !is_false!(self.state, val) {
                        index = instruction.arg(0) as usize;
                    }
                }
                Opcode::Goto => {
                    index = instruction.arg(0) as usize;
                }
                Opcode::IntegerAdd => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        add,
                        overflowing_add
                    );
                }
                Opcode::IntegerDiv => {
                    if context
                        .get_register(instruction.arg(2))
                        .is_zero_integer()
                    {
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
                Opcode::IntegerMul => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        mul,
                        overflowing_mul
                    );
                }
                Opcode::IntegerSub => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        sub,
                        overflowing_sub
                    );
                }
                Opcode::IntegerMod => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        modulo,
                        overflowing_modulo
                    );
                }
                Opcode::IntegerToFloat => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        integer::integer_to_float(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::IntegerToString => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        integer::integer_to_string(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::IntegerBitwiseAnd => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        &
                    );
                }
                Opcode::IntegerBitwiseOr => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        |
                    );
                }
                Opcode::IntegerBitwiseXor => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        ^
                    );
                }
                Opcode::IntegerShiftLeft => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_left,
                        bigint_shift_left
                    );
                }
                Opcode::IntegerShiftRight => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_right,
                        bigint_shift_right
                    );
                }
                Opcode::IntegerSmaller => {
                    integer_bool_op!(self.state, context, instruction, <);
                }
                Opcode::IntegerGreater => {
                    integer_bool_op!(self.state, context, instruction, >);
                }
                Opcode::IntegerEquals => {
                    integer_bool_op!(self.state, context, instruction, ==);
                }
                Opcode::IntegerGreaterOrEqual => {
                    integer_bool_op!(self.state, context, instruction, >=);
                }
                Opcode::IntegerSmallerOrEqual => {
                    integer_bool_op!(self.state, context, instruction, <=);
                }
                Opcode::FloatAdd => {
                    float_op!(self.state, process, context, instruction, +);
                }
                Opcode::FloatMul => {
                    float_op!(self.state, process, context, instruction, *);
                }
                Opcode::FloatDiv => {
                    float_op!(self.state, process, context, instruction, /);
                }
                Opcode::FloatSub => {
                    float_op!(self.state, process, context, instruction, -);
                }
                Opcode::FloatMod => {
                    float_op!(self.state, process, context, instruction, %);
                }
                Opcode::FloatToInteger => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        float::float_to_integer(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatToString => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        float::float_to_string(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatSmaller => {
                    float_bool_op!(self.state, context, instruction, <);
                }
                Opcode::FloatGreater => {
                    float_bool_op!(self.state, context, instruction, >);
                }
                Opcode::FloatEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = float::float_equals(&self.state, cmp, cmp_with)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatGreaterOrEqual => {
                    float_bool_op!(self.state, context, instruction, >=);
                }
                Opcode::FloatSmallerOrEqual => {
                    float_bool_op!(self.state, context, instruction, <=);
                }
                Opcode::ArraySet => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res =
                        array::array_set(&self.state, process, ary, idx, val)?;

                    context.set_register(reg, res);
                }
                Opcode::ArrayAt => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = array::array_get(&self.state, ary, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::ArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = array::array_remove(&self.state, ary, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::ArrayLength => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = array::array_length(&self.state, process, ary)?;

                    context.set_register(reg, res);
                }
                Opcode::ArrayClear => {
                    let ary = context.get_register(instruction.arg(0));

                    array::clear(ary)?;
                }
                Opcode::StringToLower => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        string::string_to_lower(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::StringToUpper => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res =
                        string::string_to_upper(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::StringEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res =
                        string::string_equals(&self.state, cmp, cmp_with)?;

                    context.set_register(reg, res);
                }
                Opcode::StringToByteArray => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = string::string_to_byte_array(
                        &self.state,
                        process,
                        val,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::StringLength => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = string::string_length(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::StringSize => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = string::string_size(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::StdoutWrite => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::stdout_write(&self.state, process, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::StderrWrite => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::stderr_write(&self.state, process, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::StdoutFlush => {
                    try_runtime_error!(
                        io::stdout_flush(),
                        self,
                        process,
                        context,
                        index
                    );
                }
                Opcode::StderrFlush => {
                    try_runtime_error!(
                        io::stderr_flush(),
                        self,
                        process,
                        context,
                        index
                    );
                }
                Opcode::StdinRead => {
                    let reg = instruction.arg(0);
                    let buf = context.get_register(instruction.arg(1));
                    let max = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::stdin_read(&self.state, process, buf, max),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileOpen => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let path = context.get_register(instruction.arg(2));
                    let mode = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        io::file_open(process, proto, path, mode),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileWrite => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let input = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::file_write(&self.state, process, file, input),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileRead => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let buf = context.get_register(instruction.arg(2));
                    let max = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        io::file_read(&self.state, process, file, buf, max),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileFlush => {
                    let file = context.get_register(instruction.arg(0));

                    try_runtime_error!(
                        io::file_flush(file),
                        self,
                        process,
                        context,
                        index
                    );
                }
                Opcode::FileSize => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::file_size(&self.state, process, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileSeek => {
                    let reg = instruction.arg(0);
                    let file = context.get_register(instruction.arg(1));
                    let pos = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::file_seek(&self.state, process, file, pos),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ModuleLoad => {
                    let reg = instruction.arg(0);
                    let name = context.get_register(instruction.arg(1));
                    let res = module::module_load(&self.state, process, name)?;

                    context.set_register(reg, res);
                    enter_context!(process, context, index);
                }
                Opcode::ModuleList => {
                    let reg = instruction.arg(0);
                    let res = module::module_list(&self.state, process);

                    context.set_register(reg, res);
                }
                Opcode::ModuleGet => {
                    let reg = instruction.arg(0);
                    let name = context.get_register(instruction.arg(1));
                    let res = module::module_get(&self.state, name)?;

                    context.set_register(reg, res);
                }
                Opcode::ModuleInfo => {
                    let reg = instruction.arg(0);
                    let module = context.get_register(instruction.arg(1));
                    let field = context.get_register(instruction.arg(2));
                    let res = module::module_info(module, field)?;

                    context.set_register(reg, res);
                }
                Opcode::SetAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = object::set_attribute(
                        &self.state,
                        process,
                        rec,
                        name,
                        val,
                    );

                    context.set_register(reg, res);
                }
                Opcode::GetAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::get_attribute(&self.state, rec, name);

                    context.set_register(reg, res);
                }
                Opcode::GetAttributeInSelf => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res =
                        object::get_attribute_in_self(&self.state, rec, name);

                    context.set_register(reg, res);
                }
                Opcode::GetPrototype => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let res = object::get_prototype(&self.state, src);

                    context.set_register(reg, res);
                }
                Opcode::LocalExists => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::local_exists(&self.state, context, idx);

                    context.set_register(reg, res);
                }
                Opcode::ProcessSpawn => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let block = context.get_register(instruction.arg(2));
                    let res = process::process_spawn(
                        &self.state,
                        process,
                        block,
                        proto,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessSendMessage => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let msg = context.get_register(instruction.arg(2));
                    let res = process::process_send_message(
                        &self.state,
                        process,
                        rec,
                        msg,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessReceiveMessage => {
                    let reg = instruction.arg(0);
                    let time = context.get_register(instruction.arg(1));

                    if let Some(message) =
                        process::process_receive_message(&self.state, process)
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

                    process::wait_for_message(&self.state, process, time)?;

                    return Ok(());
                }
                Opcode::ProcessCurrent => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let obj = process::process_current(process, proto);

                    context.set_register(reg, obj);
                }
                Opcode::ProcessSuspendCurrent => {
                    let time = context.get_register(instruction.arg(0));

                    context.instruction_index = index;

                    process::process_suspend_current(
                        &self.state,
                        process,
                        time,
                    )?;

                    return Ok(());
                }
                Opcode::SetParentLocal => {
                    let idx = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let val = context.get_register(instruction.arg(2));

                    general::set_parent_local(context, idx, depth, val)?;
                }
                Opcode::GetParentLocal => {
                    let reg = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let idx = instruction.arg(2);
                    let res = general::get_parent_local(context, idx, depth)?;

                    context.set_register(reg, res)
                }
                Opcode::ObjectEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = object::object_equals(&self.state, cmp, cmp_with);

                    context.set_register(reg, res);
                }
                Opcode::GetNil => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.nil_object,
                    );
                }
                Opcode::AttributeExists => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::attribute_exists(&self.state, src, name);

                    context.set_register(reg, res);
                }
                Opcode::GetAttributeNames => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let res =
                        object::get_attribute_names(&self.state, process, rec);

                    context.set_register(reg, res);
                }
                Opcode::TimeMonotonic => {
                    let reg = instruction.arg(0);
                    let res = time::time_monotonic(&self.state, process);

                    context.set_register(reg, res);
                }
                Opcode::RunBlock => {
                    let block = context.get_register(instruction.arg(0));
                    let start = instruction.arg(1);
                    let args = instruction.arg(2);

                    block::run_block(process, context, block, start, args)?;
                    enter_context!(process, context, index);
                }
                Opcode::SetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let val = context.get_register(instruction.arg(2));
                    let res =
                        general::set_global(&self.state, context, idx, val);

                    context.set_register(reg, res);
                }
                Opcode::GetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_global(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::Throw => {
                    let value = context.get_register(instruction.arg(0));

                    throw_value!(self, process, value, context, index);
                }
                Opcode::CopyRegister => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));

                    context.set_register(reg, val);
                }
                Opcode::TailCall => {
                    let start = instruction.arg(0);
                    let args = instruction.arg(1);

                    block::tail_call(context, start, args);
                    reset_context!(process, context, index);
                    safepoint_and_reduce!(self, process, reductions);
                }
                Opcode::CopyBlocks => {
                    let to = context.get_register(instruction.arg(0));
                    let from = context.get_register(instruction.arg(1));

                    object::copy_blocks(&self.state, to, from);
                }
                Opcode::FloatIsNan => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::float_is_nan(&self.state, ptr);

                    context.set_register(reg, res);
                }
                Opcode::FloatIsInfinite => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::float_is_infinite(&self.state, ptr);

                    context.set_register(reg, res);
                }
                Opcode::FloatFloor => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::float_floor(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatCeil => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res = float::float_ceil(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatRound => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let prec = context.get_register(instruction.arg(2));
                    let res =
                        float::float_round(&self.state, process, ptr, prec)?;

                    context.set_register(reg, res);
                }
                Opcode::DropValue => {
                    let ptr = context.get_register(instruction.arg(0));

                    object::drop_value(ptr);
                }
                Opcode::ProcessSetBlocking => {
                    let reg = instruction.arg(0);
                    let blocking = context.get_register(instruction.arg(1));
                    let res = process::process_set_blocking(
                        &self.state,
                        process,
                        blocking,
                    );

                    context.set_register(reg, res);

                    if res == self.state.false_object {
                        continue;
                    }

                    context.instruction_index = index;

                    // After this we can _not_ perform any operations on the
                    // process any more as it might be concurrently modified
                    // by the pool we just moved it to.
                    self.state.scheduler.schedule(process.clone());

                    return Ok(());
                }
                Opcode::FileRemove => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::file_remove(&self.state, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::Panic => {
                    let msg = context.get_register(instruction.arg(0));

                    return Err(msg.string_value()?.to_owned_string());
                }
                Opcode::Exit => {
                    // Any pending deferred blocks should be executed first.
                    if context
                        .schedule_deferred_blocks_of_all_parents(process)?
                    {
                        remember_and_reset!(process, context, index);
                    }

                    let status = context.get_register(instruction.arg(0));

                    general::exit(&self.state, status)?;

                    return Ok(());
                }
                Opcode::Platform => {
                    let reg = instruction.arg(0);
                    let res = env::platform(&self.state);

                    context.set_register(reg, res);
                }
                Opcode::FileCopy => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let dst = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::file_copy(&self.state, process, src, dst),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FileType => {
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
                Opcode::FileTime => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::file_time(&self.state, process, path, kind),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::TimeSystem => {
                    let reg = instruction.arg(0);
                    let res = time::time_system(&self.state, process);

                    context.set_register(reg, res);
                }
                Opcode::DirectoryCreate => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let recurse = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::directory_create(&self.state, path, recurse),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::DirectoryRemove => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let recurse = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        io::directory_remove(&self.state, path, recurse),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::DirectoryList => {
                    let reg = instruction.arg(0);
                    let path = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        io::directory_list(&self.state, process, path),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::StringConcat => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let arg = context.get_register(instruction.arg(2));
                    let res =
                        string::string_concat(&self.state, process, rec, arg)?;

                    context.set_register(reg, res);
                }
                Opcode::HasherNew => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let key0 = context.get_register(instruction.arg(2));
                    let key1 = context.get_register(instruction.arg(3));
                    let res = hasher::hasher_new(process, key0, key1, proto)?;

                    context.set_register(reg, res);
                }
                Opcode::HasherWrite => {
                    let reg = instruction.arg(0);
                    let hasher = context.get_register(instruction.arg(1));
                    let val = context.get_register(instruction.arg(2));
                    let res = hasher::hasher_write(hasher, val)?;

                    context.set_register(reg, res);
                }
                Opcode::HasherToHash => {
                    let reg = instruction.arg(0);
                    let hasher = context.get_register(instruction.arg(1));
                    let res =
                        hasher::hasher_to_hash(&self.state, process, hasher)?;

                    context.set_register(reg, res);
                }
                Opcode::Stacktrace => {
                    let reg = instruction.arg(0);
                    let limit = context.get_register(instruction.arg(1));
                    let skip = context.get_register(instruction.arg(2));
                    let res =
                        process::stacktrace(&self.state, process, limit, skip)?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessTerminateCurrent => {
                    break 'exec_loop;
                }
                Opcode::StringSlice => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let start = context.get_register(instruction.arg(2));
                    let len = context.get_register(instruction.arg(3));
                    let res = string::string_slice(
                        &self.state,
                        process,
                        string,
                        start,
                        len,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::BlockMetadata => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let field = context.get_register(instruction.arg(2));
                    let res = block::block_metadata(
                        &self.state,
                        process,
                        block,
                        field,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::StringFormatDebug => {
                    let reg = instruction.arg(0);
                    let string = context.get_register(instruction.arg(1));
                    let res = string::string_format_debug(
                        &self.state,
                        process,
                        string,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::StringConcatMultiple => {
                    let reg = instruction.arg(0);
                    let strings = context.get_register(instruction.arg(1));
                    let res = string::string_concat_multiple(
                        &self.state,
                        process,
                        strings,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayFromArray => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = byte_array::byte_array_from_array(
                        &self.state,
                        process,
                        ary,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArraySet => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = byte_array::byte_array_set(ary, idx, val)?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayAt => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res =
                        byte_array::byte_array_get(&self.state, ary, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res =
                        byte_array::byte_array_remove(&self.state, ary, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayLength => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = byte_array::byte_array_length(
                        &self.state,
                        process,
                        ary,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayClear => {
                    let array = context.get_register(instruction.arg(0));

                    byte_array::byte_array_clear(array)?;
                }
                Opcode::ByteArrayEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = byte_array::byte_array_equals(
                        &self.state,
                        cmp,
                        cmp_with,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayToString => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let drain = context.get_register(instruction.arg(2));
                    let res = byte_array::byte_array_to_string(
                        &self.state,
                        process,
                        ary,
                        drain,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::EnvGet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = env::env_get(&self.state, process, var)?;

                    context.set_register(reg, val);
                }
                Opcode::EnvSet => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let val = context.get_register(instruction.arg(2));
                    let res = env::env_set(var, val)?;

                    context.set_register(reg, res);
                }
                Opcode::EnvVariables => {
                    let reg = instruction.arg(0);
                    let res = env::env_variables(&self.state, process)?;

                    context.set_register(reg, res);
                }
                Opcode::EnvHomeDirectory => {
                    let reg = instruction.arg(0);
                    let res = env::env_home_directory(&self.state, process)?;

                    context.set_register(reg, res);
                }
                Opcode::EnvTempDirectory => {
                    let reg = instruction.arg(0);
                    let res = env::env_temp_directory(&self.state, process);

                    context.set_register(reg, res);
                }
                Opcode::EnvGetWorkingDirectory => {
                    let reg = instruction.arg(0);
                    let res = try_runtime_error!(
                        env::env_get_working_directory(&self.state, process),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::EnvSetWorkingDirectory => {
                    let reg = instruction.arg(0);
                    let dir = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        env::env_set_working_directory(dir),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::EnvArguments => {
                    let reg = instruction.arg(0);
                    let res = env::env_arguments(&self.state, process);

                    context.set_register(reg, res);
                }
                Opcode::EnvRemove => {
                    let reg = instruction.arg(0);
                    let var = context.get_register(instruction.arg(1));
                    let res = env::env_remove(&self.state, var)?;

                    context.set_register(reg, res);
                }
                Opcode::BlockGetReceiver => {
                    let reg = instruction.arg(0);
                    let res = block::block_get_receiver(context);

                    context.set_register(reg, res);
                }
                Opcode::RunBlockWithReceiver => {
                    let block = context.get_register(instruction.arg(0));
                    let rec = context.get_register(instruction.arg(1));
                    let start = instruction.arg(2);
                    let args = instruction.arg(3);

                    block::run_block_with_receiver(
                        process, context, block, rec, start, args,
                    )?;

                    enter_context!(process, context, index);
                }
                Opcode::ProcessSetPanicHandler => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let res =
                        process::process_set_panic_handler(process, block);

                    context.set_register(reg, res);
                }
                Opcode::ProcessAddDeferToCaller => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let res =
                        process::process_add_defer_to_caller(process, block)?;

                    context.set_register(reg, res);
                }
                Opcode::SetDefaultPanicHandler => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let res =
                        general::set_default_panic_handler(&self.state, block)?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessSetPinned => {
                    let reg = instruction.arg(0);
                    let pin = context.get_register(instruction.arg(1));
                    let res = process::process_set_pinned(
                        &self.state,
                        process,
                        worker,
                        pin,
                    );

                    context.set_register(reg, res);
                }
                Opcode::ProcessIdentifier => {
                    let reg = instruction.arg(0);
                    let proc = context.get_register(instruction.arg(1));
                    let res = process::process_identifier(
                        &self.state,
                        process,
                        proc,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::FFILibraryOpen => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let names = context.get_register(instruction.arg(2));
                    let res = ffi::ffi_library_open(process, names, proto)?;

                    context.set_register(reg, res);
                }
                Opcode::FFIFunctionAttach => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let lib = context.get_register(instruction.arg(2));
                    let name = context.get_register(instruction.arg(3));
                    let args = context.get_register(instruction.arg(4));
                    let rtype = context.get_register(instruction.arg(5));
                    let res = ffi::ffi_function_attach(
                        process, lib, name, args, rtype, proto,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::FFIFunctionCall => {
                    let reg = instruction.arg(0);
                    let func = context.get_register(instruction.arg(1));
                    let args = context.get_register(instruction.arg(2));
                    let proto = context.get_register(instruction.arg(3));
                    let res = ffi::ffi_function_call(
                        &self.state,
                        process,
                        func,
                        args,
                        proto,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::FFIPointerAttach => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let lib = context.get_register(instruction.arg(2));
                    let name = context.get_register(instruction.arg(3));
                    let res =
                        ffi::ffi_pointer_attach(process, lib, name, proto)?;

                    context.set_register(reg, res);
                }
                Opcode::FFIPointerRead => {
                    let reg = instruction.arg(0);
                    let ptr_proto = context.get_register(instruction.arg(1));
                    let ptr = context.get_register(instruction.arg(2));
                    let kind = context.get_register(instruction.arg(3));
                    let offset = context.get_register(instruction.arg(4));
                    let res = ffi::ffi_pointer_read(
                        &self.state,
                        process,
                        ptr_proto,
                        ptr,
                        kind,
                        offset,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::FFIPointerWrite => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let offset = context.get_register(instruction.arg(4));
                    let res = ffi::ffi_pointer_write(ptr, kind, val, offset)?;

                    context.set_register(reg, res);
                }
                Opcode::FFIPointerFromAddress => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let addr = context.get_register(instruction.arg(2));
                    let res =
                        ffi::ffi_pointer_from_address(process, addr, proto)?;

                    context.set_register(reg, res);
                }
                Opcode::FFIPointerAddress => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(instruction.arg(1));
                    let res =
                        ffi::ffi_pointer_address(&self.state, process, ptr)?;

                    context.set_register(reg, res);
                }
                Opcode::FFITypeSize => {
                    let reg = instruction.arg(0);
                    let kind = context.get_register(instruction.arg(1));
                    let res = ffi::ffi_type_size(kind)?;

                    context.set_register(reg, res);
                }
                Opcode::FFITypeAlignment => {
                    let reg = instruction.arg(0);
                    let kind = context.get_register(instruction.arg(1));
                    let res = ffi::ffi_type_alignment(kind)?;

                    context.set_register(reg, res);
                }
                Opcode::StringToInteger => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let rdx = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        string::string_to_integer(
                            &self.state,
                            process,
                            val,
                            rdx
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::StringToFloat => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = try_runtime_error!(
                        string::string_to_float(&self.state, process, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::StringByte => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = string::string_byte(val, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatToBits => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = float::float_to_bits(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::SocketCreate => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let domain = context.get_register(instruction.arg(2));
                    let kind = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_create(process, domain, kind, proto),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketWrite => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let data = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_write(&self.state, process, sock, data),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketRead => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let len = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_read(
                            &self.state,
                            process,
                            sock,
                            buff,
                            len
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketAccept => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let sock = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_accept(
                            &self.state,
                            process,
                            sock,
                            proto
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketReceiveFrom => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let len = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_receive_from(
                            &self.state,
                            process,
                            sock,
                            buff,
                            len
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketSendTo => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let buff = context.get_register(instruction.arg(2));
                    let addr = context.get_register(instruction.arg(3));
                    let port = context.get_register(instruction.arg(4));
                    let res = try_runtime_error!(
                        socket::socket_send_to(
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
                Opcode::SocketAddress => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let kind = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_address(
                            &self.state,
                            process,
                            sock,
                            kind
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketGetOption => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let opt = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_get_option(
                            &self.state,
                            process,
                            sock,
                            opt
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketSetOption => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let opt = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_set_option(&self.state, sock, opt, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketBind => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let addr = context.get_register(instruction.arg(2));
                    let port = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_bind(
                            &self.state,
                            process,
                            sock,
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
                Opcode::SocketListen => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let backlog = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_listen(sock, backlog),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::SocketConnect => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let addr = context.get_register(instruction.arg(2));
                    let port = context.get_register(instruction.arg(3));
                    let res = try_runtime_error!(
                        socket::socket_connect(
                            &self.state,
                            process,
                            sock,
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
                Opcode::SocketShutdown => {
                    let reg = instruction.arg(0);
                    let sock = context.get_register(instruction.arg(1));
                    let mode = context.get_register(instruction.arg(2));
                    let res = try_runtime_error!(
                        socket::socket_shutdown(&self.state, sock, mode),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::RandomNumber => {
                    let reg = instruction.arg(0);
                    let kind = context.get_register(instruction.arg(1));
                    let res = random::random_number(
                        &self.state,
                        process,
                        worker,
                        kind,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::RandomRange => {
                    let reg = instruction.arg(0);
                    let min = context.get_register(instruction.arg(1));
                    let max = context.get_register(instruction.arg(2));
                    let res = random::random_range(
                        &self.state,
                        process,
                        worker,
                        min,
                        max,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::RandomBytes => {
                    let reg = instruction.arg(0);
                    let size = context.get_register(instruction.arg(1));
                    let res = random::random_bytes(
                        &self.state,
                        process,
                        worker,
                        size,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::MoveResult => {
                    let reg = instruction.arg(0);
                    let res = general::move_result(process);

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

        process.terminate(&self.state);

        // Terminate once the main process has finished execution.
        if process.is_main() {
            self.state.terminate(0);
        }

        Ok(())
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    ///
    /// Returns true if a process should be suspended for garbage collection.
    fn gc_safepoint(&self, process: &RcProcess) -> bool {
        if !process.should_collect_young_generation() {
            return false;
        }

        self.state
            .gc_pool
            .schedule(Collection::new(process.clone()));

        true
    }

    fn throw(
        &self,
        process: &RcProcess,
        value: ObjectPointer,
    ) -> Result<(), String> {
        let mut deferred = Vec::new();

        process.set_result(value);

        loop {
            let context = process.context_mut();
            let code = context.code;
            let index = context.instruction_index;

            for entry in &code.catch_table.entries {
                if entry.start < index && entry.end >= index {
                    context.instruction_index = entry.jump_to;

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
        &mut self,
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
        &mut self,
        worker: &mut ProcessWorker,
        process: &RcProcess,
        message: &str,
        handler: ObjectPointer,
    ) -> Result<(), String> {
        let block = handler.block_value()?;
        let mut new_context = ExecutionContext::from_block(block);
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
        self.state.terminate(1);
    }
}
