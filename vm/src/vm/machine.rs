//! Virtual Machine for running instructions
use std::io::{self, Write, Read, Seek, SeekFrom};

use binding::Binding;
use block::Block;
use compiled_code::CompiledCodePointer;
use execution_context::ExecutionContext;
use gc::request::Request as GcRequest;
use immix::copy_object::CopyObject;
use module_registry::{ModuleRegistry, RcModuleRegistry};
use object_pointer::ObjectPointer;
use object_value;
use pool::JoinGuard as PoolJoinGuard;
use pools::{PRIMARY_POOL, SECONDARY_POOL};
use process::{RcProcess, Process};
use vm::file_open_mode;
use vm::instruction::{Instruction, InstructionType};
use vm::state::RcState;

macro_rules! reset_context {
    ($process: expr, $context: ident, $code: ident, $index: ident) => ({
        // We're storing a &mut ExecutionContext here instead of using &mut
        // Box<ExecutionContext>. This is because such a reference (as returned
        // by context()/context_mut()) will become invalid once an instruction
        // changes the current execution context.
        $context = &mut **$process.context_mut();
        $index = $context.instruction_index;
        $code = $context.code;
    });
}

macro_rules! throw_value {
    ($machine: expr, $process: expr, $value: expr, $context: ident, $code: ident, $index: ident) => ({
        $context.instruction_index = $index;

        $machine.throw($process, $value);

        reset_context!($process, $context, $code, $index);
    })
}

macro_rules! throw_io_error {
    ($machine: expr, $process: expr, $error: expr, $context: ident, $code: ident, $index: ident) => ({
        let code = $crate::error_codes::from_io_error($error);
        let value = ObjectPointer::integer(code);

        throw_value!($machine, $process, value, $context, $code, $index);
    });
}

macro_rules! throw_invalid_utf8_error {
    ($machine: expr, $process: expr, $context: ident, $code: ident, $index: ident) => ({
        let code = $crate::error_codes::STRING_INVALID_UTF8;
        let value = ObjectPointer::integer(code);

        throw_value!($machine, $process, value, $context, $code, $index);
    })
}

macro_rules! enter_context {
    ($process: expr, $context: ident, $code: ident, $index: ident) => ({
        $context.instruction_index = $index;

        reset_context!($process, $context, $code, $index);
    })
}

/// Returns a vector index for an i64
macro_rules! int_to_vector_index {
    ($vec: expr, $index: expr) => ({
        if $index >= 0 as i64 {
            $index as usize
        }
        else {
            ($vec.len() as i64 + $index) as usize
        }
    });
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
            state: state,
            module_registry: module_registry,
        }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until it returns.
    ///
    /// This method returns true if the VM terminated successfully, false
    /// otherwise.
    pub fn start(&self, file: &String) -> bool {
        let primary_guard = self.start_primary_threads();
        let gc_pool_guard = self.start_gc_threads();

        self.start_secondary_threads();
        self.start_main_process(file);

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output, so we just
        // return instead.
        if primary_guard.join().is_err() {
            return false;
        }

        if gc_pool_guard.join().is_err() {
            return false;
        }

        true
    }

    fn start_primary_threads(&self) -> PoolJoinGuard<()> {
        let machine = self.clone();
        let pool = self.state.process_pools.get(PRIMARY_POOL).unwrap();

        pool.run(move |process| machine.run(&process))
    }

    fn start_secondary_threads(&self) {
        let machine = self.clone();
        let pool = self.state.process_pools.get(SECONDARY_POOL).unwrap();

        pool.run(move |process| machine.run(&process));
    }

    /// Starts the garbage collection threads.
    fn start_gc_threads(&self) -> PoolJoinGuard<()> {
        self.state.gc_pool.run(move |request| request.perform())
    }

    fn terminate(&self) {
        self.state.process_pools.terminate();
        self.state.gc_pool.terminate();
    }

    /// Starts the main process
    pub fn start_main_process(&self, file: &String) {
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

        let pid = process_table.reserve().ok_or_else(|| {
            "No PID could be reserved".to_string()
        })?;

        let process = Process::from_block(
            pid,
            pool_id,
            block,
            self.state.global_allocator.clone(),
        );

        process_table.map(pid, process.clone());

        Ok(process)
    }

    /// Executes a single process.
    pub fn run(&self, process: &RcProcess) {
        let mut reductions = self.state.config.reductions;

        process.running();

        let mut context;
        let mut code;
        let mut index;
        let mut instruction;

        reset_context!(process, context, code, index);

        'exec_loop: loop {
            instruction = unsafe {
                // This little dance is necessary to decouple the reference to
                // the instruction from the CompiledCode reference, allowing us
                // to re-assign any of these variables whenever necessary.
                &*(code.instruction(index) as *const Instruction)
            };

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

                    context.set_register(register, code.literal(index));
                }
                // Sets an object in a register.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the object in.
                // 2. A register containing a truthy/falsy object. When the
                //    register contains a truthy object the new object will be a
                //    global object.
                // 3. An optional register containing the prototype for the
                //    object.
                InstructionType::SetObject => {
                    let register = instruction.arg(0);
                    let is_permanent_ptr =
                        context.get_register(instruction.arg(1));

                    let is_permanent = is_permanent_ptr !=
                        self.state.false_object;

                    let obj = if is_permanent {
                        self.state.permanent_allocator.lock().allocate_empty()
                    } else {
                        process.allocate_empty()
                    };

                    if let Some(proto_index) = instruction.arg_opt(2) {
                        let mut proto = context.get_register(proto_index);

                        if is_permanent && !proto.is_permanent() {
                            proto = self.state
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
                        instruction,
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
                InstructionType::GetBooleanPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.boolean_prototype,
                    );
                }
                InstructionType::GetBlockPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.block_prototype,
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

                    let cc = code.code_object(cc_index);
                    let locals = cc.locals as usize;

                    let binding = if cc.captures {
                        Binding::with_parent(context.binding.clone(), locals)
                    } else {
                        Binding::new(locals)
                    };

                    let block = Block::new(
                        cc.clone(),
                        binding,
                        process.global_scope().clone(),
                    );

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

                    let object = if let Some(register) = instruction.arg_opt(1) {
                        context.get_register(register)
                    } else {
                        self.state.nil_object
                    };

                    if block_return {
                        self.unwind_until_defining_scope(process);

                        context = &mut **process.context_mut();
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

                    if self.gc_safepoint(&process) {
                        return;
                    }

                    // Reduce once we've exhausted all the instructions in a
                    // context.
                    if reductions > 0 {
                        reductions -= 1;
                    } else {
                        self.reschedule(process.clone());
                        return;
                    }

                    reset_context!(process, context, code, index);

                    continue;
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
                    integer_op!(process, instruction, +);
                }
                // Divides an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
                InstructionType::IntegerDiv => {
                    integer_op!(process, instruction, /);
                }
                // Multiplies an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
                InstructionType::IntegerMul => {
                    integer_op!(process, instruction, *);
                }
                // Subtracts an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
                InstructionType::IntegerSub => {
                    integer_op!(process, instruction, -);
                }
                // Gets the modulo of an integer
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the left-hand side object.
                // 3. The register of the right-hand side object.
                InstructionType::IntegerMod => {
                    integer_op!(process, instruction, %);
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
                    let result = integer_ptr.integer_value().unwrap() as f64;

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
                    let integer_ptr = context.get_register(instruction.arg(1));
                    let result = integer_ptr.integer_value().unwrap().to_string();

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
                    integer_op!(process, instruction, &);
                }
                // Performs an integer bitwise OR.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerBitwiseOr => {
                    integer_op!(process, instruction, |);
                }
                // Performs an integer bitwise XOR.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerBitwiseXor => {
                    integer_op!(process, instruction, ^);
                }
                // Shifts an integer to the left.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerShiftLeft => {
                    integer_op!(process, instruction, <<);
                }
                // Shifts an integer to the right.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the integer to operate on.
                // 3. The register of the integer to use as the operand.
                InstructionType::IntegerShiftRight => {
                    integer_op!(process, instruction, >>);
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
                    integer_bool_op!(self, process, instruction, <);
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
                    integer_bool_op!(self, process, instruction, >);
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
                    integer_bool_op!(self, process, instruction, ==);
                }
                // Adds two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to add.
                InstructionType::FloatAdd => {
                    float_op!(self, process, instruction, +);
                }
                // Multiplies two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to multiply with.
                InstructionType::FloatMul => {
                    float_op!(self, process, instruction, *);
                }
                // Divides two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to divide with.
                InstructionType::FloatDiv => {
                    float_op!(self, process, instruction, /);
                }
                // Subtracts two floats
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float to subtract.
                InstructionType::FloatSub => {
                    float_op!(self, process, instruction, -);
                }
                // Gets the modulo of a float
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in.
                // 2. The register of the receiver.
                // 3. The register of the float argument.
                InstructionType::FloatMod => {
                    float_op!(self, process, instruction, %);
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
                    let result = float_ptr.float_value().unwrap() as i64;

                    context.set_register(
                        register,
                        ObjectPointer::integer(result),
                    );
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
                    let result = float_ptr.float_value().unwrap().to_string();

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
                    float_bool_op!(self, process, instruction, <);
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
                    float_bool_op!(self, process, instruction, >);
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
                    float_bool_op!(self, process, instruction, ==);
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
                InstructionType::ArrayInsert => {
                    let register = instruction.arg(0);
                    let array_ptr = context.get_register(instruction.arg(1));
                    let index_ptr = context.get_register(instruction.arg(2));
                    let value_ptr = context.get_register(instruction.arg(3));

                    let vector = array_ptr.array_value_mut().unwrap();
                    let index = int_to_vector_index!(
                        vector,
                        index_ptr.integer_value().unwrap()
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
                    let vector = array_ptr.array_value().unwrap();

                    let index = int_to_vector_index!(
                        vector,
                        index_ptr.integer_value().unwrap()
                    );

                    let value = vector.get(index).cloned().unwrap_or_else(|| {
                        self.state.nil_object
                    });

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

                    let vector = array_ptr.array_value_mut().unwrap();
                    let index = int_to_vector_index!(
                        vector,
                        index_ptr.integer_value().unwrap()
                    );

                    let value = if index > vector.len() {
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
                    let vector = array_ptr.array_value().unwrap();
                    let length = vector.len() as i64;

                    context.set_register(
                        register,
                        ObjectPointer::integer(length),
                    );
                }
                // Removes all elements from an array.
                //
                // This instruction requires 1 argument: the register of the
                // array.
                InstructionType::ArrayClear => {
                    let array_ptr = context.get_register(instruction.arg(0));
                    let vector = array_ptr.array_value_mut().unwrap();

                    vector.clear();
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
                    let lower = source_ptr.string_value().unwrap().to_lowercase();

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
                    let upper = source_ptr.string_value().unwrap().to_uppercase();

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
                    let receiver_ptr = context.get_register(instruction.arg(1));
                    let arg_ptr = context.get_register(instruction.arg(2));

                    let boolean = if receiver_ptr.string_value().unwrap() ==
                        arg_ptr.string_value().unwrap()
                    {
                        self.state.true_object
                    } else {
                        self.state.false_object
                    };

                    context.set_register(register, boolean);
                }
                // Returns an array containing the bytes of a string.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the string to get the bytes
                //    from.
                InstructionType::StringToBytes => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));

                    let array = string_ptr
                        .string_value()
                        .unwrap()
                        .as_bytes()
                        .iter()
                        .map(|&b| ObjectPointer::integer(b as i64))
                        .collect::<Vec<_>>();

                    let obj = process.allocate(
                        object_value::array(array),
                        self.state.array_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Creates a string from an array of bytes
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the result in.
                // 2. The register containing the array of bytes.
                //
                // The result of this instruction is either a string based
                // on the given bytes, or an error object.
                InstructionType::StringFromBytes => {
                    let register = instruction.arg(0);
                    let arg_ptr = context.get_register(instruction.arg(1));

                    let array = arg_ptr.array_value().unwrap();
                    let mut bytes = Vec::with_capacity(array.len());

                    for ptr in array.iter() {
                        bytes.push(ptr.integer_value().unwrap() as u8);
                    }

                    if let Ok(string) = String::from_utf8(bytes) {
                        let obj = process.allocate(
                            object_value::string(string),
                            self.state.string_prototype,
                        );

                        context.set_register(register, obj);
                    } else {
                        throw_invalid_utf8_error!(
                            self,
                            process,
                            context,
                            code,
                            index
                        );
                    }
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

                    let length =
                        arg_ptr.string_value().unwrap().chars().count() as i64;

                    context.set_register(
                        register,
                        ObjectPointer::integer(length),
                    );
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
                    let size = arg_ptr.string_value().unwrap().len() as i64;

                    context.set_register(register, ObjectPointer::integer(size));
                }
                // Writes a string to STDOUT and returns the amount of
                // written bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the string to write.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::StdoutWrite => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));
                    let string = string_ptr.string_value().unwrap();
                    let mut stdout = io::stdout();

                    let result =
                        stdout.write(string.as_bytes()).and_then(|size| {
                            stdout.flush().and_then(|_| Ok(size))
                        });

                    match result {
                        Ok(size) => {
                            let obj = ObjectPointer::integer(size as i64);

                            context.set_register(register, obj);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self,
                                process,
                                error,
                                context,
                                code,
                                index
                            );
                        }
                    }
                }
                // Writes a string to STDERR and returns the amount of
                // written bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the string to write.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::StderrWrite => {
                    let register = instruction.arg(0);
                    let string_ptr = context.get_register(instruction.arg(1));
                    let string = string_ptr.string_value().unwrap();
                    let mut stderr = io::stderr();

                    let result =
                        stderr.write(string.as_bytes()).and_then(|size| {
                            stderr.flush().and_then(|_| Ok(size))
                        });

                    match result {
                        Ok(size) => {
                            let obj = ObjectPointer::integer(size as i64);

                            context.set_register(register, obj);
                        }
                        Err(error) => {
                            throw_io_error!(
                                self,
                                process,
                                error,
                                context,
                                code,
                                index
                            );
                        }
                    }
                }
                // Reads all the data from STDIN.
                //
                // This instruction requires only one argument: the register to
                // store the read data in as a string.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::StdinRead => {
                    let register = instruction.arg(0);
                    let mut buffer = String::new();

                    if let Err(err) = io::stdin().read_to_string(&mut buffer) {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }

                    let obj = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Reads an entire line from STDIN into a string.
                //
                // This instruction requires only one argument: the register to
                // store the read data in as a string.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::StdinReadLine => {
                    let register = instruction.arg(0);
                    let mut buffer = String::new();

                    if let Err(err) = io::stdin().read_line(&mut buffer) {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }

                    let obj = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
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

                    let path = path_ptr.string_value().unwrap();
                    let mode = mode_ptr.integer_value().unwrap();
                    let open_opts = file_open_mode::options_for_integer(mode)
                        .unwrap();

                    match open_opts.open(path) {
                        Ok(file) => {
                            let obj = process.allocate_without_prototype(
                                object_value::file(file),
                            );

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(
                                self,
                                process,
                                err,
                                context,
                                code,
                                index
                            );
                        }
                    }
                }
                // Writes a string to a file.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the amount of written bytes in.
                // 2. The register containing the file object to write to.
                // 3. The register containing the string to write.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileWrite => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let string_ptr = context.get_register(instruction.arg(2));

                    let file = file_ptr.file_value_mut().unwrap();
                    let bytes = string_ptr.string_value().unwrap().as_bytes();

                    match file.write(bytes) {
                        Ok(num_bytes) => {
                            let obj = ObjectPointer::integer(num_bytes as i64);

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(
                                self,
                                process,
                                err,
                                context,
                                code,
                                index
                            );
                        }
                    }
                }
                // Reads the all data from a file.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the read data in as a string.
                // 2. The register containing the file to read from.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileRead => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let file = file_ptr.file_value_mut().unwrap();
                    let mut buffer = String::new();

                    if let Err(err) = file.read_to_string(&mut buffer) {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }

                    let obj = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Reads an entire line from a file.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the read data in as a string.
                // 2. The register containing the file to read from.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileReadLine => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let file = file_ptr.file_value_mut().unwrap();
                    let mut buffer = Vec::new();

                    for result in file.bytes() {
                        if let Ok(byte) = result {
                            buffer.push(byte);

                            if byte == 0xA {
                                break;
                            }
                        } else {
                            throw_io_error!(
                                self,
                                process,
                                result.unwrap_err(),
                                context,
                                code,
                                index
                            );

                            continue 'exec_loop;
                        }
                    }

                    // Shrink the buffer so we don't waste any additional memory
                    // that was allocated when pushing bytes.
                    buffer.shrink_to_fit();

                    if let Ok(string) = String::from_utf8(buffer) {
                        let obj = process.allocate(
                            object_value::string(string),
                            self.state.string_prototype,
                        );

                        context.set_register(register, obj);
                    } else {
                        throw_invalid_utf8_error!(
                            self,
                            process,
                            context,
                            code,
                            index
                        );
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
                    let file = file_ptr.file_value_mut().unwrap();

                    if let Err(err) = file.flush() {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }
                }
                // Returns the size of a file in bytes.
                //
                // This instruction requires two arguments:
                //
                // 1. The register to store the size of the file in.
                // 2. The register containing the file.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileSize => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let file = file_ptr.file_value().unwrap();

                    match file.metadata() {
                        Ok(meta) => {
                            let obj = ObjectPointer::integer(meta.len() as i64);

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(
                                self,
                                process,
                                err,
                                context,
                                code,
                                index
                            );
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
                    let file = file_ptr.file_value_mut().unwrap();
                    let offset = offset_ptr.integer_value().unwrap();

                    match file.seek(SeekFrom::Start(offset as u64)) {
                        Ok(cursor) => {
                            let obj = ObjectPointer::integer(cursor as i64);

                            context.set_register(register, obj);
                        }
                        Err(err) => {
                            throw_io_error!(
                                self,
                                process,
                                err,
                                context,
                                code,
                                index
                            );
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
                    let path_str = path_ptr.string_value().unwrap();

                    let (block, execute) = {
                        let mut registry = write_lock!(self.module_registry);

                        let lookup = registry
                            .get_or_set(path_str)
                            .map_err(|err| err.message())
                            .unwrap();

                        let module = lookup.module;

                        let block = Block::new(
                            module.code(),
                            Binding::new(module.code.locals()),
                            module.global_scope_ref(),
                        );

                        (block, lookup.parsed)
                    };

                    if execute {
                        self.schedule_block(
                            &block,
                            register,
                            2,
                            process,
                            instruction,
                        );

                        enter_context!(process, context, code, index);
                    } else {
                        context.set_register(register, self.state.nil_object);
                    }
                }
                InstructionType::GetBindingPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.binding_prototype,
                    );
                }
                // Gets the Binding of the current scope and sets it in a
                // register
                //
                // This instruction requires only one argument: the register
                // to store the object in.
                InstructionType::GetBinding => {
                    let register = instruction.arg(0);
                    let binding = context.binding.clone();

                    let obj = process.allocate(
                        object_value::binding(binding),
                        self.state.binding_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Sets an attribute of an object.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the written value in
                // 2. The register containing the object for which to set
                //    the attribute.
                // 3. The register containing the attribute name as a
                //    string.
                // 4. The register containing the object to set as the
                //    value.
                InstructionType::SetAttribute => {
                    let register = instruction.arg(0);
                    let target_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let value_ptr = context.get_register(instruction.arg(3));

                    if target_ptr.is_tagged_integer() {
                        panic!("attributes can not be set for integers");
                    }

                    let name = self.state.intern_pointer(&name_ptr).unwrap();

                    let value = copy_if_permanent!(
                        self.state.permanent_allocator,
                        value_ptr,
                        target_ptr
                    );

                    target_ptr.add_attribute(&process, name.clone(), value);

                    context.set_register(register, value);
                }
                // Gets an attribute from an object and stores it in a
                // register.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the attribute's value in.
                // 2. The register containing the object from which to retrieve
                //    the attribute.
                // 3. The register containing the attribute name as a string.
                //
                // If the attribute does not exist the target register is
                // set to nil.
                InstructionType::GetAttribute => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self.state.intern_pointer(&name_ptr).unwrap();

                    let method = rec_ptr
                        .lookup_attribute(&self.state, &name)
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

                    let proto =
                        source.prototype(&self.state).unwrap_or_else(
                            || self.state.nil_object,
                        );

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
                // Checks if an object responds to a message.
                //
                // This instruction requires 3 arguments:
                //
                // 1. The register to store the result in (either true or
                //    false)
                // 2. The register containing the object to check.
                // 3. The register containing the name to look up, as a
                //    string.
                InstructionType::RespondsTo => {
                    let register = instruction.arg(0);
                    let source = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self.state.intern_pointer(&name_ptr).unwrap();

                    let result = if source
                        .lookup_attribute(&self.state, &name)
                        .is_some()
                    {
                        self.state.true_object.clone()
                    } else {
                        self.state.false_object.clone()
                    };

                    context.set_register(register, result);
                }
                // Spawns a new process.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the PID in.
                // 2. The register containing the Block to run in the process.
                // 3. The register containing the ID of the process pool to schedule the
                //    process on. Defaults to the ID of the primary pool.
                InstructionType::SpawnProcess => {
                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));

                    let pool_id =
                        if let Some(pool_reg) = instruction.arg_opt(2) {
                            let ptr = context.get_register(pool_reg);

                            ptr.integer_value().unwrap() as usize
                        } else {
                            PRIMARY_POOL
                        };

                    let block_obj = block_ptr.block_value().unwrap();
                    let new_proc = self.allocate_process(pool_id, block_obj)
                        .unwrap();
                    let new_pid = new_proc.pid;

                    self.state.process_pools.schedule(new_proc);

                    context.set_register(
                        register,
                        ObjectPointer::integer(new_pid as i64),
                    );
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
                InstructionType::SendProcessMessage => {
                    let register = instruction.arg(0);
                    let pid_ptr = context.get_register(instruction.arg(1));
                    let msg_ptr = context.get_register(instruction.arg(2));
                    let pid = pid_ptr.integer_value().unwrap() as usize;

                    if let Some(receiver) = read_lock!(self.state.process_table)
                        .get(&pid)
                    {
                        receiver.send_message(&process, msg_ptr);
                    }

                    context.set_register(register, msg_ptr);
                }
                // Receives a message for the current process.
                //
                // This instruction takes 1 argument: the register to store
                // the resulting message in.
                //
                // If no messages are available the current process will be
                // suspended, and the instruction will be retried the next
                // time the process is executed.
                InstructionType::ReceiveProcessMessage => {
                    if let Some(msg_ptr) = process.receive_message() {
                        context.set_register(instruction.arg(0), msg_ptr);
                    } else {
                        context.instruction_index = index - 1;
                        self.reschedule(process.clone());

                        return;
                    }
                }
                // Gets the PID of the currently running process.
                //
                // This instruction requires one argument: the register to
                // store the PID in (as an integer).
                InstructionType::GetCurrentPid => {
                    let register = instruction.arg(0);
                    let pid = ObjectPointer::integer(process.pid as i64);

                    context.set_register(register, pid);
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
                        panic!("No binding for depth {}", depth);
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
                        panic!("No binding for depth {}", depth);
                    }
                }
                // Reads a given number of bytes from a file.
                //
                // This instruction takes 3 arguments:
                //
                // 1. The register to store the read data in as a string.
                // 2. The register containing the file to read from.
                // 3. The register containing the number of bytes to read, as a
                //    positive integer.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::FileReadExact => {
                    let register = instruction.arg(0);
                    let file_ptr = context.get_register(instruction.arg(1));
                    let size_ptr = context.get_register(instruction.arg(2));

                    let file = file_ptr.file_value_mut().unwrap();
                    let size = size_ptr.integer_value().unwrap() as usize;
                    let mut buffer = String::with_capacity(size);

                    if let Err(err) = file.take(size as u64).read_to_string(
                        &mut buffer,
                    )
                    {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }

                    let obj = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Reads a given number of bytes from STDIN.
                //
                // This instruction takes 2 arguments:
                //
                // 1. The register to store the read data in as a string.
                // 1. The register containing the number of bytes to read, as a
                //    positive integer.
                //
                // This instruction will throw when encountering an IO error.
                InstructionType::StdinReadExact => {
                    let register = instruction.arg(0);
                    let size_ptr = context.get_register(instruction.arg(1));

                    let size = size_ptr.integer_value().unwrap() as usize;
                    let mut buffer = String::with_capacity(size);
                    let stdin = io::stdin();

                    if let Err(err) = stdin.take(size as u64).read_to_string(
                        &mut buffer,
                    )
                    {
                        throw_io_error!(self, process, err, context, code, index);
                        continue;
                    }

                    let obj = process.allocate(
                        object_value::string(buffer),
                        self.state.string_prototype,
                    );

                    context.set_register(register, obj);
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
                // Sets the top-level object in a register.
                //
                // This instruction requires one argument: the register to
                // store the object in.
                InstructionType::GetToplevel => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.top_level,
                    );
                }
                InstructionType::GetNilPrototype => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.nil_prototype,
                    );
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
                // 3. The register containing the attribute name as a
                //    string.
                InstructionType::AttrExists => {
                    let register = instruction.arg(0);
                    let source_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self.state.intern_pointer(&name_ptr).unwrap();

                    let obj = if source_ptr
                        .lookup_attribute(&self.state, &name)
                        .is_some()
                    {
                        self.state.true_object.clone()
                    } else {
                        self.state.false_object.clone()
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
                // 3. The register containing the attribute name as a string.
                //
                // If the attribute did not exist the target register is set
                // to nil instead.
                InstructionType::RemoveAttribute => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self.state.intern_pointer(&name_ptr).unwrap();

                    if rec_ptr.is_tagged_integer() {
                        panic!("attributes can not be removed for integers");
                    }

                    let obj = if let Some(attribute) =
                        rec_ptr.get_mut().remove_attribute(&name)
                    {
                        attribute
                    } else {
                        self.state.nil_object
                    };

                    context.set_register(register, obj);
                }
                // Gets all the attributes available on an object.
                //
                // This instruction requires 2 arguments:
                //
                // 1. The register to store the attributes in.
                // 2. The register containing the object for which to get
                //    all attributes.
                InstructionType::GetAttributes => {
                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let attributes = rec_ptr.attributes();

                    let obj = process.allocate(
                        object_value::array(attributes),
                        self.state.array_prototype,
                    );

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
                // Gets the current value of a monotonic clock in
                // nanoseconds.
                //
                // This instruction requires one argument: the register to
                // set the time in, as an integer.
                InstructionType::MonotonicTimeNanoseconds => {
                    let register = instruction.arg(0);
                    let duration = self.state.start_time.elapsed();
                    let nsec = (duration.as_secs() * 1000000000) +
                        duration.subsec_nanos() as u64;

                    context.set_register(
                        register,
                        ObjectPointer::integer(nsec as i64),
                    );
                }
                // Gets the current value of a monotonic clock in
                // milliseconds.
                //
                // This instruction requires one argument: the register to
                // set the time in, as a float.
                InstructionType::MonotonicTimeMilliseconds => {
                    let register = instruction.arg(0);
                    let duration = self.state.start_time.elapsed();

                    let msec = (duration.as_secs() * 1_000) as f64 +
                        duration.subsec_nanos() as f64 / 1_000_000.0;

                    let obj = process.allocate(
                        object_value::float(msec),
                        self.state.float_prototype,
                    );

                    context.set_register(register, obj);
                }
                // Executes a Block object.
                //
                // This instruction takes the following arguments:
                //
                // 1. The register to store the return value in.
                // 2. The register containing the Block object to run.
                //
                // Any extra arguments passed are passed as arguments to the
                // block.
                InstructionType::RunBlock => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let block_ptr = context.get_register(instruction.arg(1));
                    let block = block_ptr.block_value().unwrap();

                    self.schedule_block(
                        &block,
                        register,
                        2,
                        process,
                        instruction,
                    );

                    enter_context!(process, context, code, index);
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
                        self.state.permanent_allocator.lock().copy_object(object)
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
                // Sends a message to an object.
                //
                // This instruction takes the following arguments:
                //
                // 1. The register to store the method's return value in.
                // 2. The register containing the receiver.
                // 3. The register containing the message name as a string.
                //
                // Any additional arguments are passed as arguments to the
                // method.
                InstructionType::SendMessage => {
                    context.line = instruction.line;

                    let register = instruction.arg(0);
                    let rec_ptr = context.get_register(instruction.arg(1));
                    let name_ptr = context.get_register(instruction.arg(2));
                    let name = self.state.intern_pointer(&name_ptr).unwrap();
                    let method_opt = rec_ptr.lookup_attribute(&self.state, &name);

                    let method = if let Some(found) = method_opt {
                        found
                    } else {
                        panic!(
                            "the object in register {} does not respond to {:?}",
                            instruction.arg(1),
                            name.string_value().unwrap()
                        );
                    };

                    let block = method.block_value().unwrap();

                    self.schedule_block(
                        &block,
                        register,
                        3,
                        process,
                        instruction,
                    );

                    enter_context!(process, context, code, index);
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

                    throw_value!(self, process, value, context, code, index);
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
                // This instruction takes 0 or more arguments, each containing
                // a register to set in the corresponding local variable.
                InstructionType::TailCall => {
                    context.binding.locals_mut().reset();

                    self.prepare_new_context(
                        process,
                        instruction,
                        &code,
                        context,
                        0,
                    );

                    context.register.values.reset();

                    index = 0;
                }
            };
        }

        process.finished();

        write_lock!(self.state.process_table).release(&process.pid);

        // Terminate once the main process has finished execution.
        if process.is_main() {
            self.terminate();
        }
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
            self.schedule_gc_request(
                GcRequest::heap(self.state.clone(), process.clone()),
            );

            true
        } else if process.should_collect_mailbox() {
            self.schedule_gc_request(
                GcRequest::mailbox(self.state.clone(), process.clone()),
            );

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

    fn schedule_block(
        &self,
        block: &Block,
        return_register: usize,
        arg_offset: usize,
        process: &RcProcess,
        instruction: &Instruction,
    ) {
        let context = ExecutionContext::from_block(block, Some(return_register));

        self.prepare_new_context(
            process,
            instruction,
            &block.code,
            &context,
            arg_offset,
        );

        process.push_context(context);
    }

    fn prepare_new_context(
        &self,
        process: &RcProcess,
        instruction: &Instruction,
        code: &CompiledCodePointer,
        context: &ExecutionContext,
        arg_offset: usize,
    ) {
        let ins_args_count = instruction.arguments.len();
        let arg_count = ins_args_count - arg_offset;

        if !code.valid_number_of_arguments(arg_count) {
            panic!(
                "{} takes {} arguments but {} were supplied",
                code.name,
                code.label_for_number_of_arguments(),
                arg_count
            );
        }

        let locals = context.binding.locals_mut();
        let max_index = arg_offset + code.number_of_arguments_to_set(arg_count);
        let total = code.arguments();

        for index in arg_offset..max_index {
            let local_index = index - arg_offset;

            locals[local_index] = process.get_register(instruction.arg(index));
        }

        // We can only reach this code if a rest argument is defined, thus
        // we only need to check if too many arguments are passed.
        if arg_count > total {
            let rest_args = (max_index..ins_args_count)
                .map(|index| process.get_register(instruction.arg(index)))
                .collect::<Vec<ObjectPointer>>();

            locals[total + 1] = process.allocate(
                object_value::array(rest_args),
                self.state.array_prototype,
            );
        }
    }

    fn throw(&self, process: &RcProcess, value: ObjectPointer) {
        loop {
            let code = process.compiled_code();
            let context = process.context_mut();
            let index = context.instruction_index;

            for entry in code.catch_table.entries.iter() {
                if entry.start < index && entry.end >= index {
                    context.instruction_index = entry.jump_to;
                    context.set_register(entry.register, value);

                    return;
                }
            }

            if process.pop_context() {
                panic!(
                    "A thrown value reached the top-level in process {}",
                    process.pid
                );
            }
        }
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
