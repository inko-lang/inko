//! Virtual Machine for running instructions
use float_cmp::ApproxEqUlps;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use rayon::ThreadPoolBuilder;
use std::f64;
use std::fs;
use std::i32;
use std::i64;
use std::io::{self, Seek, SeekFrom, Write};
use std::ops::{Add, Mul, Sub};
use std::thread;

use binding::Binding;
use block::Block;
use byte_array;
use compiled_code::CompiledCodePointer;
use date_time::DateTime;
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
use pool::{JoinGuard as PoolJoinGuard, STACK_SIZE};
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
            $vm.reschedule($process.clone());
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
            .num_threads(self.state.config.generic_parallel_threads)
            .stack_size(STACK_SIZE)
            .build_global()
            .unwrap();
    }

    fn start_primary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(PRIMARY_POOL).unwrap();

        pool.run(move |process| machine.run_with_error_handling(&process))
    }

    fn start_secondary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(SECONDARY_POOL).unwrap();

        pool.run(move |process| machine.run_with_error_handling(&process))
    }

    fn start_suspension_worker(&self) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        let builder = thread::Builder::new()
            .stack_size(STACK_SIZE)
            .name("suspend worker".to_string());

        builder
            .spawn(move || {
                state.suspension_list.process_suspended_processes(&state)
            }).unwrap()
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> PoolJoinGuard<()> {
        self.state.gc_pool.run(move |mut request| request.perform())
    }

    pub fn start_finalizer_threads(&self) -> PoolJoinGuard<()> {
        self.state
            .finalizer_pool
            .run(move |mut block| block.finalize_pending())
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
                Binding::new(code.locals()),
                module.global_scope_ref(),
            );

            self.allocate_process(PRIMARY_POOL, &block).unwrap()
        };

        self.state.process_pools.schedule(process);
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(
        &self,
        pool_id: usize,
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
    pub fn run_with_error_handling(&self, process: &RcProcess) {
        if let Err(message) = self.run(process) {
            self.panic(process, &message);
        }
    }

    /// Executes a single process.
    #[cfg_attr(feature = "cargo-clippy", allow(cyclomatic_complexity))]
    pub fn run(&self, process: &RcProcess) -> Result<(), String> {
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
                // Sets a literal value in a register.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the literal value in.
                // 2. The index to the value in the literals table of the
                //    current compiled code object.
                InstructionType::SetLiteral => {
                    let register = instruction.arg(0);
                    let index = instruction.arg(1);
                    let literal = unsafe { context.code.literal(index) };

                    context.set_register(register, literal);
                }
                // Sets an object in a register.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the object in.
                // 2. A register containing a truthy/falsy object. When the
                //    register contains a truthy object the new object will be a
                //    permanent object.
                // 3. An optional register containing the prototype for the
                //    object.
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
                // Sets an array in a register.
                //
                // This instruction requires at least one argument: the register
                // to store the resulting array in. Any extra instruction
                // arguments should point to registers containing objects to
                // store in the array.
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
                // Sets a "true" value in a register.
                //
                // This instruction requires only one argument: the register to
                // store the object in.
                InstructionType::GetTrue => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.true_object,
                    );
                }
                // Sets a "false" value in a register.
                //
                // This instruction requires only one argument: the register to
                // store the object in.
                InstructionType::GetFalse => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.false_object,
                    );
                }
                // Sets a local variable to a given register's value.
                //
                // This instruction requires two arguments:
                //
                // 1. The local variable index to set.
                // 2. The register containing the object to store in the
                //    variable.
                InstructionType::SetLocal => {
                    let local_index = instruction.arg(0);
                    let object = context.get_register(instruction.arg(1));

                    context.set_local(local_index, object);
                }
                // Gets a local variable and stores it in a register.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the local's value in.
                // 2. The local variable index to get the value from.
                InstructionType::GetLocal => {
                    let register = instruction.arg(0);
                    let local_index = instruction.arg(1);
                    let object = context.get_local(local_index);

                    context.set_register(register, object);
                }
                // Sets a Block in a register.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the object in.
                // 2. The index of the CompiledCode object literal to use for
                //    creating the Block.
                //
                // If the underlying CompiledCode object captures any outer
                // locals the block's binding will have its parent set to the
                // binding of the current context.
                //
                // A block that captures local variables can not be safely
                // stored in a global object as this can result in the captured
                // locals outliving the process they were allocated in.
                InstructionType::SetBlock => {
                    let register = instruction.arg(0);
                    let cc_index = instruction.arg(1);

                    let cc = context.code.code_object(cc_index);
                    let locals = cc.locals as usize;

                    let binding = if cc.captures {
                        context.binding.clone()
                    } else {
                        Binding::new(locals)
                    };

                    let block =
                        Block::new(cc, binding, *process.global_scope());

                    let obj = process.allocate(
                        object_value::block(block),
                        self.state.block_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Returns the value in the given register.
                //
                // This instruction takes two arguments:
                //
                // 1. An integer that indicates if we're performing a regular
                //    return (0) or a block return (1).
                // 2. The register containing the value to return. If no value
                //    is given nil will be returned instead.
                //
                // When performing a block return we'll first unwind the call
                // stack to the scope that defined the current block.
                InstructionType::Return => {
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
                            parent_context.set_register(register, object);
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
                // Jumps to an instruction if a register is not set or set
                // to false.
                //
                // This instruction takes two arguments:
                //
                // 1. The instruction index to jump to if a register is not set.
                // 2. The register to check.
                InstructionType::GotoIfFalse => {
                    let value_reg = instruction.arg(1);

                    if is_false!(self, context.get_register(value_reg)) {
                        index = instruction.arg(0);
                    }
                }
                // Jumps to an instruction if a register is set.
                //
                // This instruction takes two arguments:
                //
                // 1. The instruction index to jump to if a register is set.
                // 2. The register to check.
                InstructionType::GotoIfTrue => {
                    let value_reg = instruction.arg(1);

                    if !is_false!(self, context.get_register(value_reg)) {
                        index = instruction.arg(0);
                    }
                }
                // Jumps to a specific instruction.
                //
                // This instruction takes one argument: the instruction index to
                // jump to.
                InstructionType::Goto => {
                    index = instruction.arg(0);
                }
                // Adds two integers
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
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
                // Divides an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
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
                // Multiplies an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
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
                // Subtracts an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
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
                // Gets the modulo of an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
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
                // Converts an integer to a float
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to convert.
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
                // Converts an integer to a string
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to convert.
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
                // Performs an integer bitwise AND.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerBitwiseAnd => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        &
                    );
                }
                // Performs an integer bitwise OR.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerBitwiseOr => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        |
                    );
                }
                // Performs an integer bitwise XOR.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerBitwiseXor => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        ^
                    );
                }
                // Shifts an integer to the left.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
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
                // Shifts an integer to the right.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
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
                // Checks if one integer is smaller than the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the integer to compare.
                // 3. The register containing the integer to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::IntegerSmaller => {
                    integer_bool_op!(self.state, context, instruction, <);
                }
                // Checks if one integer is greater than the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the integer to compare.
                // 3. The register containing the integer to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::IntegerGreater => {
                    integer_bool_op!(self.state, context, instruction, >);
                }
                // Checks if two integers are equal.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the integer to compare.
                // 3. The register containing the integer to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::IntegerEquals => {
                    integer_bool_op!(self.state, context, instruction, ==);
                }
                // Checks if one integer is greater than or requal to the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the integer to compare.
                // 3. The register containing the integer to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::IntegerGreaterOrEqual => {
                    integer_bool_op!(self.state, context, instruction, >=);
                }
                // Checks if one integer is smaller than or requal to the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the integer to compare.
                // 3. The register containing the integer to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::IntegerSmallerOrEqual => {
                    integer_bool_op!(self.state, context, instruction, <=);
                }
                // Adds two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to add.
                InstructionType::FloatAdd => {
                    float_op!(self.state, process, instruction, +);
                }
                // Multiplies two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to multiply with.
                InstructionType::FloatMul => {
                    float_op!(self.state, process, instruction, *);
                }
                // Divides two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to divide with.
                InstructionType::FloatDiv => {
                    float_op!(self.state, process, instruction, /);
                }
                // Subtracts two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to subtract.
                InstructionType::FloatSub => {
                    float_op!(self.state, process, instruction, -);
                }
                // Gets the modulo of a float
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float argument.
                InstructionType::FloatMod => {
                    float_op!(self.state, process, instruction, %);
                }
                // Converts a float to an integer
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the float to convert.
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
                // Converts a float to a string
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the float to convert.
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
                // Checks if one float is smaller than the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to compare.
                // 3. The register containing the float to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::FloatSmaller => {
                    float_bool_op!(self.state, context, instruction, <);
                }
                // Checks if one float is greater than the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to compare.
                // 3. The register containing the float to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::FloatGreater => {
                    float_bool_op!(self.state, context, instruction, >);
                }
                // Checks if two floats are equal.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to compare.
                // 3. The register containing the float to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
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
                // Checks if one float is greater than or requal to the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to compare.
                // 3. The register containing the float to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::FloatGreaterOrEqual => {
                    float_bool_op!(self.state, context, instruction, >=);
                }
                // Checks if one float is smaller than or requal to the other.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to compare.
                // 3. The register containing the float to compare with.
                //
                // The result of this instruction is either boolean true or
                // false.
                InstructionType::FloatSmallerOrEqual => {
                    float_bool_op!(self.state, context, instruction, <=);
                }
                // Inserts a value in an array.
                //
                // This instruction requires 4 arguments:
                //
                // 1. The register to store the result (the inserted value)
                //    in.
                // 2. The register containing the array to insert into.
                // 3. The register containing the index (as an integer) to
                //    insert at.
                // 4. The register containing the value to insert.
                //
                // If an index is out of bounds the array is filled with nil
                // values. A negative index can be used to indicate a
                // position from the end of the array.
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
                // Gets the value of an array index.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the value in.
                // 2. The register containing the array.
                // 3. The register containing the index.
                //
                // This instruction will set nil in the target register if
                // the array index is out of bounds. A negative index can be
                // used to indicate a position from the end of the array.
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
                // Removes a value from an array.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the removed value in.
                // 2. The register containing the array to remove a value
                //    from.
                // 3. The register containing the index.
                //
                // This instruction sets nil in the target register if the
                // index is out of bounds. A negative index can be used to
                // indicate a position from the end of the array.
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
                // Gets the amount of elements in an array.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the length in.
                // 2. The register containing the array.
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
                // Removes all elements from an array.
                //
                // This instruction requires 1 argument: the register of the
                // array.
                InstructionType::ArrayClear => {
                    let array_ptr = context.get_register(instruction.arg(0));

                    array_ptr.array_value_mut()?.clear();
                }
                // Returns the lowercase equivalent of a string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the new string in.
                // 2. The register containing the input string.
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
                // Returns the uppercase equivalent of a string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the new string in.
                // 2. The register containing the input string.
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
                // Checks if two strings are equal.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the string to compare.
                // 3. The register of the string to compare with.
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
                // Returns a byte array containing the bytes of a given string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the string to get the bytes
                //    from.
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
                // Returns the amount of characters in a string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the string.
                InstructionType::StringLength => {
                    let register = instruction.arg(0);
                    let arg_ptr = context.get_register(instruction.arg(1));
                    let length = process.allocate_usize(
                        arg_ptr.string_value()?.chars().count(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, length);
                }
                // Returns the amount of bytes in a string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the string.
                InstructionType::StringSize => {
                    let register = instruction.arg(0);
                    let arg_ptr = context.get_register(instruction.arg(1));
                    let size = process.allocate_usize(
                        arg_ptr.string_value()?.len(),
                        self.state.integer_prototype,
                    );

                    context.set_register(register, size);
                }
                // Writes a string to STDOUT and returns the amount of
                // written bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the string or byte array to write.
                //
                // This instruction will throw when encountering an IO error.
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
                // Flushes all output to STDOUT.
                //
                // This instruction takes one argument: a register to set to nil
                // if the output was flushed successfully.
                //
                // This instruction will throw when encountering an IO error.
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
                // Writes a string to STDERR and returns the amount of
                // written bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the string or byte array to write.
                //
                // This instruction will throw when encountering an IO error.
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
                // Flushes all output to STDERR.
                //
                // This instruction takes one argument: a register to set to nil
                // if the output was flushed successfully.
                //
                // This instruction will throw when encountering an IO error.
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
                // Reads all the data from STDIN.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the number of read bytes in.
                // 2. The register containing the byte array to read the data
                //    into.
                // 3. The register containing the number of bytes to read. If
                //    set to nil, all remaining data is read.
                //
                // This instruction will throw when encountering an IO error.
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
                // Opens a file handle in a particular mode (read-only,
                // write-only, etc).
                //
                // This instruction requires X arguments:
                //
                // 1. The register to store the file object in.
                // 2. The path to the file to open.
                // 3. The register containing an integer that specifies the file
                //    open mode.
                //
                // The available file modes supported are as follows:
                //
                // * 0: read-only
                // * 1: write-only
                // * 2: append-only
                // * 3: read+write
                // * 4: read+append
                //
                // This instruction will throw when encountering an IO error.
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
                // Writes a string to a file.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the file object to write to.
                // 3. The register containing the string or byte array to write.
                //
                // This instruction will throw when encountering an IO error.
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
                // Reads data from a file into an array of bytes.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the number of read bytes in.
                // 2. The register containing the file to read from.
                // 3. The register containing the byte array to read the data
                //    into.
                // 4. The register containing the number of bytes to read. If
                //    set to nil, all remaining data is read.
                //
                // This instruction will throw when encountering an IO error.
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
                // Flushes a file.
                //
                // This instruction requires one argument: the register
                // containing the file to flush.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileFlush => {
                    let file_ptr = context.get_register(instruction.arg(0));
                    let file = file_ptr.file_value_mut()?;

                    if let Err(err) = file.flush() {
                        throw_io_error!(self, process, err, context, index);
                    }
                }
                // Returns the size of a file in bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the size of the file in.
                // 2. The register containing the path to the file.
                //
                // This instruction will throw when encountering an IO error.
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
                // Sets a file cursor to the given offset in bytes.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the new cursor position in.
                // 2. The register containing the input file.
                // 3. The offset to seek to as an integer. This integer must be
                //    greater than 0.
                //
                // This instruction will throw when encountering an IO error.
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
                // Loads a bytecode module and executes it.
                //
                // A module is only executed the first time it is loaded, after
                // that this instruction acts like a no-op.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the result in. The first time a
                //    module is loaded this will be set to whatever the module
                //    returned, after that it will be set to nil.
                // 2. A register containing the file path to the module, as a
                //    string.
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
                            Binding::new(module.code.locals()),
                            module.global_scope_ref(),
                        );

                        (block, lookup.parsed)
                    };

                    if execute {
                        let new_context = ExecutionContext::from_block(
                            &block,
                            Some(register),
                        );

                        process.push_context(new_context);

                        enter_context!(process, context, index);
                    } else {
                        context.set_register(register, self.state.nil_object);
                    }
                }
                // Sets an attribute of an object.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the written value in
                // 2. The register containing the object for which to set
                //    the attribute.
                // 3. The register containing the attribute name.
                // 4. The register containing the object to set as the
                //    value.
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
                // Sets the attribute of an object to an empty object, but only
                // if the attribute is not already set.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the object set in.
                // 2. The register containing the object to store the attribute
                //    in.
                // 3. The register containing the name of the attribute.
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
                // Gets an attribute from an object and stores it in a
                // register.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the attribute's value in.
                // 2. The register containing the object from which to retrieve
                //    the attribute.
                // 3. The register containing the attribute name.
                //
                // If the attribute does not exist the target register is
                // set to nil.
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
                // Sets the prototype of an object.
                //
                // This instruction requires two arguments:
                //
                // 1. The register containing the object for which to set
                //    the prototype.
                // 2. The register containing the object to use as the
                //    prototype.
                InstructionType::SetPrototype => {
                    let source = context.get_register(instruction.arg(0));
                    let proto = context.get_register(instruction.arg(1));

                    source.get_mut().set_prototype(proto);
                }
                // Gets the prototype of an object.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the prototype in.
                // 2. The register containing the object to get the
                //    prototype from.
                //
                // If no prototype was found, nil is set in the register
                // instead.
                InstructionType::GetPrototype => {
                    let register = instruction.arg(0);
                    let source = context.get_register(instruction.arg(1));

                    let proto = source
                        .prototype(&self.state)
                        .unwrap_or_else(|| self.state.nil_object);

                    context.set_register(register, proto);
                }
                // Checks if a local variable exists.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in (true or false).
                // 2. The local variable index to check.
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
                // Spawns a new process.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the PID in.
                // 2. The register containing the Block to run in the process.
                // 3. The register containing the ID of the process pool to schedule the
                //    process on. Defaults to the ID of the primary pool.
                InstructionType::ProcessSpawn => {
                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));

                    let pool_id = if let Some(reg) = instruction.arg_opt(2) {
                        context.get_register(reg).usize_value()?
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
                // Sends a message to a process.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the message in.
                // 2. The register containing the PID to send the message
                //    to.
                // 3. The register containing the message (an object) to
                //    send to the process.
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
                // Receives a message for the current process.
                //
                // This instruction takes two arguments:
                //
                // 1. The register to store the received message in.
                // 2. A timeout after which the process will resume,
                //    even if no message is received. If the register is set to
                //    nil or the value is negative the timeout is ignored.
                //
                // If no messages are available the current process will be
                // suspended, and the instruction will be retried the next
                // time the process is executed.
                //
                // If a timeout is given that expires the given register will be
                // set to nil.
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
                // Gets the PID of the currently running process.
                //
                // This instruction requires one argument: the register to
                // store the PID in (as an integer).
                InstructionType::ProcessCurrentPid => {
                    let register = instruction.arg(0);
                    let pid = process.allocate_usize(
                        process.pid,
                        self.state.integer_prototype,
                    );

                    context.set_register(register, pid);
                }
                // Gets the status of the given process as an integer.
                //
                // This instruction takes two arguments:
                //
                // 1. The register to store the status in.
                // 2. The register containing the PID of the process to check.
                InstructionType::ProcessStatus => {
                    let register = instruction.arg(0);
                    let pid_ptr = process.get_register(instruction.arg(1));
                    let pid = pid_ptr.usize_value()?;
                    let table = read_lock!(self.state.process_table);

                    let status = if let Some(receiver) = table.get(pid) {
                        receiver.status_integer()
                    } else {
                        ProcessStatus::Finished as usize
                    };

                    let status_ptr = process
                        .allocate_usize(status, self.state.integer_prototype);

                    context.set_register(register, status_ptr);
                }
                // Suspends the current process.
                //
                // This instruction takes one argument: a register
                // containing the minimum amount of time (as an integer) the
                // process should be suspended. If the register is set to nil or
                // contains a negative value the timeout is ignored.
                InstructionType::ProcessSuspendCurrent => {
                    let time_ptr = context.get_register(instruction.arg(0));
                    let timeout = optional_timeout!(time_ptr);

                    context.instruction_index = index;

                    self.state
                        .suspension_list
                        .suspend(process.clone(), timeout);

                    return Ok(());
                }
                // Sets a local variable in one of the parent bindings.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The local variable index to set.
                // 2. The number of parent bindings to traverse in order to
                //    find the binding to set the variable in.
                // 3. The register containing the value to set.
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
                // Gets a local variable in one of the parent bindings.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the local variable in.
                // 2. The number of parent bindings to traverse in order to
                //    find the binding to get the variable from.
                // 3. The local variable index to get.
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
                // Checks if two objects are equal.
                //
                // Comparing equality is done by simply comparing the
                // addresses of both pointers: if they're equal then the
                // objects are also considered to be equal.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the object to compare.
                // 3. The register containing the object to compare with.
                //
                // The result of this instruction is either boolean true, or
                // false.
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
                // Checks if one object is a kind of another object.
                //
                // An object is considered a kind of another object when the
                // object compared with is in the prototype chain of the object
                // we're comparing.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in as a boolean.
                // 2. The register containing the object to compare.
                // 3. The register containing the object to compare with.
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
                // Checks if an object's attribute contains the given value.
                // This instruction will walk the prototype chain until a match
                // is found or we run out of objects.
                //
                // This instruction requires 4 attributes:
                //
                // 1. The register to set the result to as a boolean.
                // 2. The object whos prototype chain to check.
                // 3. The name of the attribute to check.
                // 4. The value to check in the attribute.
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
                            source.get().lookup_attribute_in_self(name)
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
                // Sets the top-level object in a register.
                //
                // This instruction requires one argument: the register to
                // store the object in.
                InstructionType::GetToplevel => {
                    context
                        .set_register(instruction.arg(0), self.state.top_level);
                }
                // Sets the nil singleton in a register.
                //
                // This instruction requires only one argument: the register
                // to store the object in.
                InstructionType::GetNil => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.nil_object,
                    );
                }
                // Checks if an attribute exists in an object.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in (true or false).
                // 2. The register containing the object to check.
                // 3. The register containing the attribute name.
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
                // Removes a attribute from an object.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the removed attribute in.
                // 2. The register containing the object from which to
                //    remove the attribute.
                // 3. The register containing the attribute name.
                //
                // If the attribute did not exist the target register is set
                // to nil instead.
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
                // Gets all the attributes names available on an object.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the attribute names in.
                // 2. The register containing the object for which to get
                //    all attributes names.
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
                // Gets the current value of a monotonic clock in seconds.
                //
                // This instruction requires one argument: the register to
                // set the time in, as a float.
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
                // Executes a Block object.
                //
                // This instruction takes the following arguments:
                //
                // 1. The register to store the return value in.
                // 2. The register containing the Block object to run.
                // 3. An integer indicating the number of positional arguments.
                // 4. An integer indicating the number of keyword arguments.
                // 5. A variable list of positional arguments.
                // 6. A variable list of keyword argument and value pairs. The
                //    keyword argument names must be interned strings.
                InstructionType::RunBlock => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let block = block_ptr.block_value()?;

                    let new_ctx =
                        ExecutionContext::from_block(&block, Some(register));

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
                // Sets a global variable to a given register's value.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the written value in.
                // 2. The global variable index to set.
                // 3. The register containing the object to store in the
                //    variable.
                //
                // If the object being stored is not a permanent object it will
                // be copied to the permanent generation.
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
                // Gets a global variable and stores it in a register.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the global's value in.
                // 2. The global variable index to get the value from.
                InstructionType::GetGlobal => {
                    let register = instruction.arg(0);
                    let index = instruction.arg(1);
                    let object = process.get_global(index);

                    context.set_register(register, object);
                }
                // Throws a value
                //
                // This instruction requires one arguments: the register
                // containing the value to throw.
                //
                // This method will unwind the call stack until either the
                // value is caught, or until we reach the top level (at
                // which point we terminate the VM).
                InstructionType::Throw => {
                    let value = context.get_register(instruction.arg(0));

                    throw_value!(self, process, value, context, index);
                }
                // Sets a register to the value of another register.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to set.
                // 2. The register to get the value from.
                InstructionType::SetRegister => {
                    let value = context.get_register(instruction.arg(1));

                    context.set_register(instruction.arg(0), value);
                }
                // Performs a tail call on the current block.
                //
                // This instruction takes the same arguments as RunBlock, except
                // for the register and block arguments.
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
                // Copies all of the blocks of one object into another object.
                // Only blocks defined directly on the source object will be
                // copied.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register containing the object to copy the blocks to.
                // 2. The register containing the object to copy the blocks
                //    from.
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
                // Sets a register to true if a given float register is a NaN
                // value.
                //
                // This instruction takes 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to check.
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
                // Sets a register to true if a given float register is an
                // infinite number.
                //
                // This instruction takes 2 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the float to check.
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
                // Gets the floor of a float.
                //
                // This instruction takes 2 arguments:
                //
                // 1. The register to store the result in as a float.
                // 2. The register containing the float.
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
                // Gets the ceiling of a float.
                //
                // This instruction takes 2 arguments:
                //
                // 1. The register to store the result in as a float.
                // 2. The register containing the float.
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
                // Rounds a float to the nearest number.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the result in as a float.
                // 2. The register containing the float.
                // 3. The register containing an integer indicating the number
                //    of decimals to round to.
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
                // Immediately drops the value of an object, if any.
                //
                // This instruction takes one argument: the register containing
                // the object for which to drop the value.
                //
                // If the object has no value this instruction won't do
                // anything.
                //
                // Once dropped the value of the object should no longer be used
                // as its memory may have been deallocated.
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
                // Moves the current process to the given pool.
                //
                // This instruction takes one argument: the register containing
                // the pool ID to move to.
                //
                // If the process is already running in the given pool this
                // instruction does nothing.
                InstructionType::MoveToPool => {
                    let pool_ptr = context.get_register(instruction.arg(0));
                    let pool_id = pool_ptr.usize_value()?;

                    if !self.state.process_pools.pool_id_is_valid(pool_id) {
                        return Err(format!(
                            "The process pool ID {} is invalid",
                            pool_id
                        ));
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
                // Removes a file.
                //
                // This instruction takes two arguments:
                //
                // 1. The register to store the result in. This register will be
                //    set to nil upon success.
                // 2. The register containing the path to the file to remove.
                //
                // This instruction will throw when encountering an IO error.
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
                // Produces a VM panic.
                //
                // A VM panic will result in a stack trace and error message
                // being displayed, after which the VM will terminate.
                //
                // This instruction requires one argument: the register
                // containing the error message to display.
                InstructionType::Panic => {
                    context.line = instruction.line;

                    let message_ptr = context.get_register(instruction.arg(0));

                    return Err(message_ptr.string_value()?.clone());
                }
                // Terminates the VM with a given exit status.
                //
                // This instruction takes one argument: a register containing an
                // integer to use for the exit status.
                InstructionType::Exit => {
                    let status_ptr = context.get_register(instruction.arg(0));
                    let status = status_ptr.i32_value()?;

                    self.state.set_exit_status(status);
                    self.terminate();

                    return Ok(());
                }
                // Returns the type of the platform as an integer.
                //
                // This instruction requires one argument: a register to store
                // the resulting platform ID in.
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
                // Copies a file from one location to another.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the number of copied bytes in as an
                //    integer.
                // 2. The register containing the file path to copy.
                // 3. The register containing the new path of the file.
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
                // Gets the file type of a path.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in as an integer.
                // 2. The register containing the path to check.
                //
                // This instruction can produce the following values:
                //
                // 1. `0`: the path does not exist.
                // 2. `1`: the path is a file.
                // 3. `2`: the path is a directory.
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
                // Gets the creation, modification or access time of a file.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the result in as a float.
                // 2. The register containing the file path of the file.
                // 3. The register containing an integer indicating what kind of
                //    timestamp to retrieve.
                //
                // This instruction will throw an error message (as a String) if
                // the file's metadata could not be retrieved.
                //
                // This instruction will panic if the timestamp kind is invalid.
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
                // Gets the current system time.
                //
                // This instruction takes one argument: the register to store
                // the number of seconds since the Unix epoch in seconds
                // (including fractional seconds), as a Float.
                InstructionType::TimeSystem => {
                    let register = instruction.arg(0);
                    let timestamp = DateTime::now().timestamp();
                    let pointer = process.allocate(
                        object_value::float(timestamp),
                        self.state.float_prototype,
                    );

                    context.set_register(register, pointer);
                }
                // Gets the system time's offset to UTC in seconds.
                //
                // This instruction takes one argument: the register to store
                // the offset in as an integer.
                InstructionType::TimeSystemOffset => {
                    let register = instruction.arg(0);
                    let offset =
                        ObjectPointer::integer(DateTime::now().utc_offset());

                    context.set_register(register, offset);
                }
                // Determines if DST is active or not.
                //
                // This instruction requires one argument: the register to store
                // the result in as a boolean.
                InstructionType::TimeSystemDst => {
                    let register = instruction.arg(0);
                    let result = if DateTime::now().dst_active() {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, result);
                }
                // Creates a new directory.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in, which is always
                //    `nil`.
                // 2. The register containing the path to create.
                // 3. A register containing a boolean. When set to `true` the
                //    path is created recursively.
                //
                // This instruction may throw an IO error.
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
                // Removes an existing directory.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in, which is always
                //    `nil`.
                // 2. The register containing the path to remove.
                // 3. A register containing a boolean. When set to `true` the
                //    contents of the directory are removed before removing the
                //    directory itself.
                //
                // This instruction may throw an IO error.
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
                // Lists the contents of a directory.
                //
                // This instruction requirs two arguments:
                //
                // 1. The register to store the result in, as an Array of
                //    Strings.
                // 2. The register containing the path to the directory.
                //
                // This instruction may throw an IO error.
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
                // Concatenates two strings together, producing a new one.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the first string.
                // 3. The register containing the second string.
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
                // Creates a new hasher.
                //
                // This instruction requires only one argument: the register to
                // store the object in.
                InstructionType::HasherNew => {
                    let register = instruction.arg(0);
                    let pointer = process.allocate(
                        object_value::hasher(Hasher::new()),
                        self.state.object_prototype,
                    );

                    context.set_register(register, pointer);
                }
                // Hashes an object
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the result in, this is always `nil`.
                // 2. The register containing the hasher to use.
                // 3. The register containing the object to hash.
                //
                // The following objects can be hashed:
                //
                // 1. Integers
                // 2. Big integers
                // 3. Floats
                // 4. Strings
                // 5. Permanent objects
                InstructionType::HasherWrite => {
                    let register = instruction.arg(0);
                    let mut hasher_ptr =
                        context.get_register(instruction.arg(1));

                    let val_ptr = context.get_register(instruction.arg(2));

                    val_ptr.hash_object(hasher_ptr.hasher_value_mut()?)?;

                    context.set_register(register, self.state.nil_object);
                }
                // Returns the hash for the values written to a hasher.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in as an integer.
                // 2. The register containing the hasher to fetch the result
                //    from.
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
                // Produces a stack trace.
                //
                // This instruction requires the three arguments:
                //
                // 1. The register to store the trace in.
                // 2. A register containing the maximum number of frames to
                //    include. If set to nil all frames will be included.
                // 3. A register containing the number of call frames to skip
                //    (from the start of the stack).
                //
                // The trace is stored as an array of arrays. Each sub array
                // contains:
                //
                // 1. The path of the file being executed.
                // 2. The name of the ExecutionContext.
                // 3. The line of the ExecutionContext.
                //
                // The frames are returned in reverse order. This means that the
                // most recent call frame is the last value in the array.
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
                // Terminates the current process.
                //
                // This instruction does not take any arguments.
                InstructionType::ProcessTerminateCurrent => {
                    break 'exec_loop;
                }
                // Slices a string into a new string.
                //
                // Slicing operates on the _characters_ of a string, not the
                // bytes.
                //
                // This instruction requires four arguments:
                //
                // 1. The register to store the new string in.
                // 2. The register containing the string to slice.
                // 3. The register containing the start position.
                // 4. The register containing the number of values to include.
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
                // Obtains metadata from a block.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the block to obtain the data from.
                // 3. The register containing an integer describing what kind of
                //    information to obtain.
                //
                // The following kinds of metadata are available:
                //
                // * 0: The name of the block.
                // * 1: The file path of the block.
                // * 2: The line number of the block.
                // * 3: The argument names of the block.
                // * 4: The number of required arguments.
                // * 5: A boolean indicating if the last argument is a rest
                //      argument.
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
                // Formats a string for debugging purposes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in, as a string.
                // 2. The register containing the string to format.
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
                // Takes an Array of String objects and concatenates them
                // together efficiently.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the resulting String in.
                // 2. The register containing the Array of Strings.
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
                // Creates a new byte array from an Array of integers.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing an array of integers to use for
                //    creating the byte array.
                //
                // This instruction will panic if any of the bytes is not in the
                // range 0..256.
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
                // Inserts a value into a byte array.
                //
                // This instruction requires four arguments:
                //
                // 1. The register to store the written value in, as an integer.
                // 2. The register containing the byte array to write to.
                // 3. The register containing the index to store the byte at.
                // 4. The register containing the integer to store in the byte
                //    array.
                //
                // This instruction will panic if any of the bytes is not in the
                // range 0..256.
                //
                // Unlike ArraySet, this instruction will panic if the index is
                // out of bounds.
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
                // Returns the value at the given position in a byte array.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the value in.
                // 2. The register containing the byte array to retrieve the
                //    value from.
                // 3. The register containing the value index.
                //
                // This instruction will set the target register to nil if no
                // value was found.
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
                // Removes a value from a byte array.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the removed value in.
                // 2. The register containing the byte array to remove a value
                //    from.
                // 3. The register containing the index of the value to remove.
                //
                // This instruction will set the target register to nil if no
                // value was removed.
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
                // Gets the amount of elements in a byte array.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the length in.
                // 2. The register containing the byte array.
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
                // Removes all elements from a byte array.
                //
                // This instruction only requires one argument: the register
                // containing the byte array to clear.
                InstructionType::ByteArrayClear => {
                    let array_ptr = context.get_register(instruction.arg(0));

                    array_ptr.byte_array_value_mut()?.clear();
                }
                // Checks two byte arrays for equality.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the result in as a boolean.
                // 2. The register containing the byte array to compare.
                // 3. The register containing the byte array to compare with.
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
                // Converts a byte array to a string.
                //
                // This instruction requires three arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the byte array to convert.
                // 3. The register containing a boolean indicating if the input
                //    array should be drained.
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
                // Sets the prototype of booleans in a register.
                //
                // This instruction only requires one argument: the register to
                // store the prototype in.
                InstructionType::GetBooleanPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.boolean_prototype,
                    );
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

    /// Reschedules a process.
    fn reschedule(&self, process: RcProcess) {
        self.state.process_pools.schedule(process);
    }

    fn schedule_gc_request(&self, request: GcRequest) {
        request.process.suspend_for_gc();
        self.state.gc_pool.schedule(request);
    }

    fn schedule_gc_for_finished_process(&self, process: &RcProcess) {
        let request = GcRequest::finished(self.state.clone(), process.clone());
        self.state.gc_pool.schedule(request);
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
        loop {
            let code = process.compiled_code();
            let context = process.context_mut();
            let index = context.instruction_index;

            for entry in &code.catch_table.entries {
                if entry.start < index && entry.end >= index {
                    context.instruction_index = entry.jump_to;
                    context.set_register(entry.register, value);

                    return Ok(());
                }
            }

            if process.pop_context() {
                return Err(format!(
                    "A thrown value reached the top-level in process {}",
                    process.pid
                ));
            }
        }
    }

    fn panic(&self, process: &RcProcess, message: &str) {
        runtime_panic::display_panic(process, message);

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
