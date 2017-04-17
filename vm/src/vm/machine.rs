//! Virtual Machine for running instructions
use binding::Binding;
use block::Block;
use gc::request::Request as GcRequest;
use module_registry::{ModuleRegistry, RcModuleRegistry};
use object_pointer::ObjectPointer;
use pool::JoinGuard as PoolJoinGuard;
use pools::{PRIMARY_POOL, SECONDARY_POOL};
use process::{RcProcess, Process};
use vm::instruction::{Instruction, InstructionType};
use vm::instructions::array;
use vm::instructions::binding;
use vm::instructions::block;
use vm::instructions::boolean;
use vm::instructions::code_execution;
use vm::instructions::constant;
use vm::instructions::control_flow;
use vm::instructions::error;
use vm::instructions::file;
use vm::instructions::float;
use vm::instructions::globals;
use vm::instructions::integer;
use vm::instructions::literals;
use vm::instructions::local_variable;
use vm::instructions::method;
use vm::instructions::nil;
use vm::instructions::object;
use vm::instructions::process;
use vm::instructions::prototype;
use vm::instructions::stderr;
use vm::instructions::stdin;
use vm::instructions::stdout;
use vm::instructions::string;
use vm::instructions::time;
use vm::state::RcState;

macro_rules! suspend_retry {
    ($machine: expr, $context: expr, $process: expr, $index: expr) => ({
        $context.instruction_index = $index - 1;
        $machine.reschedule($process.clone());

        return;
    })
}

macro_rules! enter_context {
    ($context: expr, $index: expr, $label: tt) => ({
        $context.instruction_index = $index;
        continue $label;
    })
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

            let module = registry.parse_path(file)
                .map_err(|err| err.message())
                .unwrap();

            let code = module.code();
            let block = Block::new(code,
                                   Binding::new(code.locals()),
                                   module.global_scope_ref());

            self.allocate_process(PRIMARY_POOL, &block).unwrap()
        };

        self.state.process_pools.schedule(process);
    }

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(&self,
                            pool_id: usize,
                            block: &Block)
                            -> Result<RcProcess, String> {
        let mut process_table = write_lock!(self.state.process_table);

        let pid = process_table.reserve()
            .ok_or_else(|| "No PID could be reserved".to_string())?;

        let process = Process::from_block(pid,
                                          pool_id,
                                          block,
                                          self.state.global_allocator.clone());

        process_table.map(pid, process.clone());

        Ok(process)
    }

    /// Executes a single process.
    fn run(&self, process: &RcProcess) {
        let mut reductions = self.state.config.reductions;

        process.running();

        'exec_loop: loop {
            let code = process.compiled_code();
            let count = code.instructions.len();

            // We're storing a &mut ExecutionContext here instead of using &mut
            // Box<ExecutionContext>. This is because such a reference (as
            // returned by context()/context_mut()) will become invalid once an
            // instruction changes the current execution context.
            let mut context = &mut **process.context_mut();
            let mut index = context.instruction_index;

            while index < count {
                let instruction = code.instruction(index);

                index += 1;

                match instruction.instruction_type {
                    InstructionType::SetLiteral => {
                        literals::set_literal(process, &code, instruction);
                    }
                    InstructionType::SetObject => {
                        object::set_object(self, process, instruction);
                    }
                    InstructionType::SetArray => {
                        array::set_array(self, process, instruction);
                    }
                    InstructionType::GetIntegerPrototype => {
                        prototype::get_integer_prototype(self,
                                                         process,
                                                         instruction);
                    }
                    InstructionType::GetFloatPrototype => {
                        prototype::get_float_prototype(self,
                                                       process,
                                                       instruction);
                    }
                    InstructionType::GetStringPrototype => {
                        prototype::get_string_prototype(self,
                                                        process,
                                                        instruction);
                    }
                    InstructionType::GetArrayPrototype => {
                        prototype::get_array_prototype(self,
                                                       process,
                                                       instruction);
                    }
                    InstructionType::GetTruePrototype => {
                        prototype::get_true_prototype(self, process, instruction);
                    }
                    InstructionType::GetFalsePrototype => {
                        prototype::get_false_prototype(self,
                                                       process,
                                                       instruction);
                    }
                    InstructionType::GetMethodPrototype => {
                        prototype::get_method_prototype(self,
                                                        process,
                                                        instruction);
                    }
                    InstructionType::GetBlockPrototype => {
                        prototype::get_block_prototype(self,
                                                       process,
                                                       instruction);
                    }
                    InstructionType::GetTrue => {
                        boolean::get_true(self, process, instruction);
                    }
                    InstructionType::GetFalse => {
                        boolean::get_false(self, process, instruction);
                    }
                    InstructionType::SetLocal => {
                        local_variable::set_local(process, instruction);
                    }
                    InstructionType::GetLocal => {
                        local_variable::get_local(process, instruction);
                    }
                    InstructionType::SetBlock => {
                        block::set_block(self, process, &code, instruction);
                    }
                    InstructionType::Return => {
                        control_flow::return_value(process, instruction);

                        break;
                    }
                    InstructionType::GotoIfFalse => {
                        index = control_flow::goto_if_false(self,
                                                            process,
                                                            instruction,
                                                            index);
                    }
                    InstructionType::GotoIfTrue => {
                        index = control_flow::goto_if_true(self,
                                                           process,
                                                           instruction,
                                                           index);
                    }
                    InstructionType::Goto => {
                        index = control_flow::goto(instruction);
                    }
                    InstructionType::DefMethod => {
                        method::def_method(self, process, instruction);
                    }
                    InstructionType::IsError => {
                        error::is_error(self, process, instruction);
                    }
                    InstructionType::IntegerAdd => {
                        integer::integer_add(process, instruction);
                    }
                    InstructionType::IntegerDiv => {
                        integer::integer_div(process, instruction);
                    }
                    InstructionType::IntegerMul => {
                        integer::integer_mul(process, instruction);
                    }
                    InstructionType::IntegerSub => {
                        integer::integer_sub(process, instruction);
                    }
                    InstructionType::IntegerMod => {
                        integer::integer_mod(process, instruction);
                    }
                    InstructionType::IntegerToFloat => {
                        integer::integer_to_float(self, process, instruction);
                    }
                    InstructionType::IntegerToString => {
                        integer::integer_to_string(self, process, instruction);
                    }
                    InstructionType::IntegerBitwiseAnd => {
                        integer::integer_bitwise_and(process, instruction);
                    }
                    InstructionType::IntegerBitwiseOr => {
                        integer::integer_bitwise_or(process, instruction);
                    }
                    InstructionType::IntegerBitwiseXor => {
                        integer::integer_bitwise_xor(process, instruction);
                    }
                    InstructionType::IntegerShiftLeft => {
                        integer::integer_shift_left(process, instruction);
                    }
                    InstructionType::IntegerShiftRight => {
                        integer::integer_shift_right(process, instruction);
                    }
                    InstructionType::IntegerSmaller => {
                        integer::integer_smaller(self, process, instruction);
                    }
                    InstructionType::IntegerGreater => {
                        integer::integer_greater(self, process, instruction);
                    }
                    InstructionType::IntegerEquals => {
                        integer::integer_equals(self, process, instruction);
                    }
                    InstructionType::FloatAdd => {
                        float::float_add(self, process, instruction);
                    }
                    InstructionType::FloatMul => {
                        float::float_mul(self, process, instruction);
                    }
                    InstructionType::FloatDiv => {
                        float::float_div(self, process, instruction);
                    }
                    InstructionType::FloatSub => {
                        float::float_sub(self, process, instruction);
                    }
                    InstructionType::FloatMod => {
                        float::float_mod(self, process, instruction);
                    }
                    InstructionType::FloatToInteger => {
                        float::float_to_integer(process, instruction);
                    }
                    InstructionType::FloatToString => {
                        float::float_to_string(self, process, instruction);
                    }
                    InstructionType::FloatSmaller => {
                        float::float_smaller(self, process, instruction);
                    }
                    InstructionType::FloatGreater => {
                        float::float_greater(self, process, instruction);
                    }
                    InstructionType::FloatEquals => {
                        float::float_equals(self, process, instruction);
                    }
                    InstructionType::ArrayInsert => {
                        array::array_insert(self, process, instruction);
                    }
                    InstructionType::ArrayAt => {
                        array::array_at(self, process, instruction);
                    }
                    InstructionType::ArrayRemove => {
                        array::array_remove(self, process, instruction);
                    }
                    InstructionType::ArrayLength => {
                        array::array_length(process, instruction);
                    }
                    InstructionType::ArrayClear => {
                        array::array_clear(process, instruction);
                    }
                    InstructionType::StringToLower => {
                        string::string_to_lower(self, process, instruction);
                    }
                    InstructionType::StringToUpper => {
                        string::string_to_upper(self, process, instruction);
                    }
                    InstructionType::StringEquals => {
                        string::string_equals(self, process, instruction);
                    }
                    InstructionType::StringToBytes => {
                        string::string_to_bytes(self, process, instruction);
                    }
                    InstructionType::StringFromBytes => {
                        string::string_from_bytes(self, process, instruction);
                    }
                    InstructionType::StringLength => {
                        string::string_length(process, instruction);
                    }
                    InstructionType::StringSize => {
                        string::string_size(process, instruction);
                    }
                    InstructionType::StdoutWrite => {
                        stdout::stdout_write(process, instruction);
                    }
                    InstructionType::StderrWrite => {
                        stderr::stderr_write(process, instruction);
                    }
                    InstructionType::StdinRead => {
                        stdin::stdin_read(self, process, instruction);
                    }
                    InstructionType::StdinReadLine => {
                        stdin::stdin_read_line(self, process, instruction);
                    }
                    InstructionType::FileOpen => {
                        file::file_open(process, instruction);
                    }
                    InstructionType::FileWrite => {
                        file::file_write(process, instruction);
                    }
                    InstructionType::FileRead => {
                        file::file_read(self, process, instruction);
                    }
                    InstructionType::FileReadLine => {
                        file::file_read_line(self, process, instruction);
                    }
                    InstructionType::FileFlush => {
                        file::file_flush(process, instruction);
                    }
                    InstructionType::FileSize => {
                        file::file_size(process, instruction);
                    }
                    InstructionType::FileSeek => {
                        file::file_seek(process, instruction);
                    }
                    InstructionType::ParseFile => {
                        code_execution::parse_file(self, process, instruction);
                    }
                    InstructionType::FileParsed => {
                        code_execution::file_parsed(self, process, instruction);
                    }
                    InstructionType::GetBindingPrototype => {
                        prototype::get_binding_prototype(self,
                                                         process,
                                                         instruction);
                    }
                    InstructionType::GetBinding => {
                        binding::get_binding(self, process, instruction);
                    }
                    InstructionType::SetConstant => {
                        constant::set_const(self, process, instruction);
                    }
                    InstructionType::GetConstant => {
                        constant::get_const(self, process, instruction);
                    }
                    InstructionType::SetAttribute => {
                        object::set_attr(self, process, instruction);
                    }
                    InstructionType::GetAttribute => {
                        object::get_attr(self, process, instruction);
                    }
                    InstructionType::SetPrototype => {
                        prototype::set_prototype(process, instruction);
                    }
                    InstructionType::GetPrototype => {
                        prototype::get_prototype(self, process, instruction);
                    }
                    InstructionType::LocalExists => {
                        local_variable::local_exists(self, process, instruction);
                    }
                    InstructionType::RespondsTo => {
                        method::responds_to(self, process, instruction);
                    }
                    InstructionType::SpawnProcess => {
                        process::spawn_process(self, process, instruction);
                    }
                    InstructionType::SendProcessMessage => {
                        process::send_process_message(self, process, instruction);
                    }
                    InstructionType::ReceiveProcessMessage => {
                        let suspend =
                            process::receive_process_message(process,
                                                             instruction);

                        if suspend {
                            suspend_retry!(self, context, process, index);
                        }
                    }
                    InstructionType::GetCurrentPid => {
                        process::get_current_pid(process, instruction);
                    }
                    InstructionType::SetParentLocal => {
                        local_variable::set_parent_local(process, instruction);
                    }
                    InstructionType::GetParentLocal => {
                        local_variable::get_parent_local(process, instruction);
                    }
                    InstructionType::ErrorToInteger => {
                        error::error_to_integer(process, instruction);
                    }
                    InstructionType::FileReadExact => {
                        file::file_read_exact(self, process, instruction);
                    }
                    InstructionType::StdinReadExact => {
                        stdin::stdin_read_exact(self, process, instruction);
                    }
                    InstructionType::ObjectEquals => {
                        object::object_equals(self, process, instruction);
                    }
                    InstructionType::GetToplevel => {
                        object::get_toplevel(self, process, instruction);
                    }
                    InstructionType::GetNilPrototype => {
                        prototype::get_nil_prototype(self, process, instruction);
                    }
                    InstructionType::GetNil => {
                        nil::get_nil(self, process, instruction);
                    }
                    InstructionType::LookupMethod => {
                        method::lookup_method(self, process, instruction);
                    }
                    InstructionType::AttrExists => {
                        object::attr_exists(self, process, instruction);
                    }
                    InstructionType::ConstExists => {
                        constant::const_exists(self, process, instruction);
                    }
                    InstructionType::RemoveMethod => {
                        method::remove_method(self, process, instruction);
                    }
                    InstructionType::RemoveAttribute => {
                        object::remove_attribute(self, process, instruction);
                    }
                    InstructionType::GetMethods => {
                        method::get_methods(self, process, instruction);
                    }
                    InstructionType::GetMethodNames => {
                        method::get_method_names(self, process, instruction);
                    }
                    InstructionType::GetAttributes => {
                        object::get_attributes(self, process, instruction);
                    }
                    InstructionType::GetAttributeNames => {
                        object::get_attribute_names(self, process, instruction);
                    }
                    InstructionType::MonotonicTimeNanoseconds => {
                        time::monotonic_time_nanoseconds(self,
                                                         process,
                                                         instruction);
                    }
                    InstructionType::MonotonicTimeMilliseconds => {
                        time::monotonic_time_milliseconds(self,
                                                          process,
                                                          instruction);
                    }
                    InstructionType::RunBlock => {
                        code_execution::run_block(process, instruction);

                        enter_context!(context, index, 'exec_loop);
                    }
                    InstructionType::RunBlockWithRest => {
                        code_execution::run_block_with_rest();

                        enter_context!(context, index, 'exec_loop);
                    }
                    InstructionType::GetGlobal => {
                        globals::get_global(process, instruction);
                    }
                    InstructionType::SetGlobal => {
                        globals::set_global(process, instruction);
                    }
                    InstructionType::SendMessage => {
                        code_execution::send_message(self, process, instruction);

                        enter_context!(context, index, 'exec_loop);
                    }
                    InstructionType::ArrayPush => {
                        array::array_push(self, process, instruction);
                    }
                    InstructionType::Throw => {
                        context.instruction_index = index;

                        error::throw(process, instruction);

                        continue 'exec_loop;
                    }
                };
            } // while

            // Once we're at the top-level _and_ we have no more instructions to
            // process we'll bail out of the main execution loop.
            if process.pop_context() {
                break;
            }

            // The underlying ExecutionContext is no longer available at this
            // point. Rust however is not aware of this due to the use of the
            // LocalData structure in Process.
            drop(context);

            if self.gc_safepoint(&process) {
                return;
            }

            // Reduce once we've exhausted all the instructions in a context.
            if reductions > 0 {
                reductions -= 1;
            } else {
                self.reschedule(process.clone());
                return;
            }
        } // loop

        process.finished();

        write_lock!(self.state.process_table).release(&process.pid);

        // Terminate once the main process has finished execution.
        if process.is_main() {
            self.terminate();
        }
    }

    /// Collects a set of arguments from an instruction.
    pub fn collect_arguments(&self,
                             process: &RcProcess,
                             instruction: &Instruction,
                             offset: usize,
                             amount: usize)
                             -> Vec<ObjectPointer> {
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
            self.schedule_gc_request(GcRequest::heap(self.state.clone(),
                                                     process.clone()));

            true
        } else if process.should_collect_mailbox() {
            self.schedule_gc_request(GcRequest::mailbox(self.state.clone(),
                                                        process.clone()));

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
}
