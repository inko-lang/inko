//! Virtual Machine for running instructions
use block::Block;
use compiled_code::RcCompiledCode;
use file_registry::{FileRegistry, RcFileRegistry};
use gc::request::Request as GcRequest;
use object_pointer::ObjectPointer;
use object_value;
use process::{RcProcess, Process};
use pool::JoinGuard as PoolJoinGuard;
use pools::{PRIMARY_POOL, SECONDARY_POOL};
use vm::instruction::{Instruction, InstructionType};
use vm::state::RcState;
use vm::instructions::array;
use vm::instructions::binding;
use vm::instructions::block;
use vm::instructions::boolean;
use vm::instructions::code_execution;
use vm::instructions::constant;
use vm::instructions::error;
use vm::instructions::file;
use vm::instructions::float;
use vm::instructions::control_flow;
use vm::instructions::integer;
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
    pub file_registry: RcFileRegistry,
}

impl Machine {
    /// Creates a new Machine with various fields set to their defaults.
    pub fn default(state: RcState) -> Self {
        let file_registry = FileRegistry::with_rc(state.clone());

        Machine::new(state, file_registry)
    }

    pub fn new(state: RcState, file_registry: RcFileRegistry) -> Self {
        Machine {
            state: state,
            file_registry: file_registry,
        }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until it returns.
    ///
    /// This method returns true if the VM terminated successfully, false
    /// otherwise.
    pub fn start(&self, code: RcCompiledCode) -> bool {
        let primary_guard = self.start_primary_threads();
        let gc_pool_guard = self.start_gc_threads();

        self.start_secondary_threads();

        let main_process = self.allocate_process(PRIMARY_POOL, code).unwrap();

        self.state.process_pools.schedule(main_process);

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

    /// Allocates a new process and returns the PID and Process structure.
    pub fn allocate_process(&self,
                            pool_id: usize,
                            code: RcCompiledCode)
                            -> Result<RcProcess, String> {
        let mut process_table = write_lock!(self.state.process_table);

        let pid = process_table.reserve()
            .ok_or_else(|| "No PID could be reserved".to_string())?;

        let process = Process::from_code(pid,
                                         pool_id,
                                         code,
                                         self.state.global_allocator.clone());

        process_table.map(pid, process.clone());

        Ok(process)
    }

    pub fn allocate_method(&self,
                           process: &RcProcess,
                           receiver: &ObjectPointer,
                           block_ref: &Box<Block>)
                           -> ObjectPointer {
        let block = (**block_ref).clone();
        let value = object_value::block(block);
        let proto = self.state.method_prototype.clone();

        if receiver.is_permanent() {
            self.state
                .permanent_allocator
                .lock()
                .allocate_with_prototype(value, proto)
        } else {
            process.allocate(value, proto)
        }
    }

    /// Executes a single process.
    fn run(&self, process: &RcProcess) {
        let mut reductions = self.state.config.reductions;

        process.running();

        'exec_loop: loop {
            let code = process.compiled_code();
            let mut index = process.instruction_index();
            let count = code.instructions.len();

            // We're storing a &mut ExecutionContext here instead of using &mut
            // Box<ExecutionContext>. This is because such a reference (as
            // returned by context()/context_mut()) will become invalid once an
            // instruction changes the current execution context.
            let mut context = &mut **process.context_mut();

            while index < count {
                let ref instruction = code.instructions[index];

                index += 1;

                match instruction.instruction_type {
                    InstructionType::SetInteger => {
                        integer::set_integer(self, process, &code, instruction);
                    }
                    InstructionType::SetFloat => {
                        float::set_float(self, process, &code, instruction);
                    }
                    InstructionType::SetString => {
                        string::set_string(self, process, &code, instruction);
                    }
                    InstructionType::SetObject => {
                        object::set_object(self, process, &code, instruction);
                    }
                    InstructionType::SetArray => {
                        array::set_array(self, process, &code, instruction);
                    }
                    InstructionType::GetIntegerPrototype => {
                        prototype::get_integer_prototype(self,
                                                         process,
                                                         &code,
                                                         instruction);
                    }
                    InstructionType::GetFloatPrototype => {
                        prototype::get_float_prototype(self,
                                                       process,
                                                       &code,
                                                       instruction);
                    }
                    InstructionType::GetStringPrototype => {
                        prototype::get_string_prototype(self,
                                                        process,
                                                        &code,
                                                        instruction);
                    }
                    InstructionType::GetArrayPrototype => {
                        prototype::get_array_prototype(self,
                                                       process,
                                                       &code,
                                                       instruction);
                    }
                    InstructionType::GetTruePrototype => {
                        prototype::get_true_prototype(self,
                                                      process,
                                                      &code,
                                                      instruction);
                    }
                    InstructionType::GetFalsePrototype => {
                        prototype::get_false_prototype(self,
                                                       process,
                                                       &code,
                                                       instruction);
                    }
                    InstructionType::GetMethodPrototype => {
                        prototype::get_method_prototype(self,
                                                        process,
                                                        &code,
                                                        instruction);
                    }
                    InstructionType::GetBlockPrototype => {
                        prototype::get_block_prototype(self,
                                                       process,
                                                       &code,
                                                       instruction);
                    }
                    InstructionType::GetTrue => {
                        boolean::get_true(self, process, &code, instruction);
                    }
                    InstructionType::GetFalse => {
                        boolean::get_false(self, process, &code, instruction);
                    }
                    InstructionType::SetLocal => {
                        local_variable::set_local(self,
                                                  process,
                                                  &code,
                                                  instruction);
                    }
                    InstructionType::GetLocal => {
                        local_variable::get_local(self,
                                                  process,
                                                  &code,
                                                  instruction);
                    }
                    InstructionType::SetBlock => {
                        block::set_block(self, process, &code, instruction);
                    }
                    InstructionType::Return => {
                        control_flow::return_value(self,
                                                   process,
                                                   &code,
                                                   instruction);

                        break;
                    }
                    InstructionType::GotoIfFalse => {
                        index = control_flow::goto_if_false(self,
                                                            process,
                                                            &code,
                                                            instruction,
                                                            index);
                    }
                    InstructionType::GotoIfTrue => {
                        index = control_flow::goto_if_true(self,
                                                           process,
                                                           &code,
                                                           instruction,
                                                           index);
                    }
                    InstructionType::Goto => {
                        index =
                            control_flow::goto(self, process, &code, instruction);
                    }
                    InstructionType::DefMethod => {
                        method::def_method(self, process, &code, instruction);
                    }
                    InstructionType::IsError => {
                        error::is_error(self, process, &code, instruction);
                    }
                    InstructionType::IntegerAdd => {
                        integer::integer_add(self, process, &code, instruction);
                    }
                    InstructionType::IntegerDiv => {
                        integer::integer_div(self, process, &code, instruction);
                    }
                    InstructionType::IntegerMul => {
                        integer::integer_mul(self, process, &code, instruction);
                    }
                    InstructionType::IntegerSub => {
                        integer::integer_sub(self, process, &code, instruction);
                    }
                    InstructionType::IntegerMod => {
                        integer::integer_mod(self, process, &code, instruction);
                    }
                    InstructionType::IntegerToFloat => {
                        integer::integer_to_float(self,
                                                  process,
                                                  &code,
                                                  instruction);
                    }
                    InstructionType::IntegerToString => {
                        integer::integer_to_string(self,
                                                   process,
                                                   &code,
                                                   instruction);
                    }
                    InstructionType::IntegerBitwiseAnd => {
                        integer::integer_bitwise_and(self,
                                                     process,
                                                     &code,
                                                     instruction);
                    }
                    InstructionType::IntegerBitwiseOr => {
                        integer::integer_bitwise_or(self,
                                                    process,
                                                    &code,
                                                    instruction);
                    }
                    InstructionType::IntegerBitwiseXor => {
                        integer::integer_bitwise_xor(self,
                                                     process,
                                                     &code,
                                                     instruction);
                    }
                    InstructionType::IntegerShiftLeft => {
                        integer::integer_shift_left(self,
                                                    process,
                                                    &code,
                                                    instruction);
                    }
                    InstructionType::IntegerShiftRight => {
                        integer::integer_shift_right(self,
                                                     process,
                                                     &code,
                                                     instruction);
                    }
                    InstructionType::IntegerSmaller => {
                        integer::integer_smaller(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::IntegerGreater => {
                        integer::integer_greater(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::IntegerEquals => {
                        integer::integer_equals(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::FloatAdd => {
                        float::float_add(self, process, &code, instruction);
                    }
                    InstructionType::FloatMul => {
                        float::float_mul(self, process, &code, instruction);
                    }
                    InstructionType::FloatDiv => {
                        float::float_div(self, process, &code, instruction);
                    }
                    InstructionType::FloatSub => {
                        float::float_sub(self, process, &code, instruction);
                    }
                    InstructionType::FloatMod => {
                        float::float_mod(self, process, &code, instruction);
                    }
                    InstructionType::FloatToInteger => {
                        float::float_to_integer(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::FloatToString => {
                        float::float_to_string(self, process, &code, instruction);
                    }
                    InstructionType::FloatSmaller => {
                        float::float_smaller(self, process, &code, instruction);
                    }
                    InstructionType::FloatGreater => {
                        float::float_greater(self, process, &code, instruction);
                    }
                    InstructionType::FloatEquals => {
                        float::float_equals(self, process, &code, instruction);
                    }
                    InstructionType::ArrayInsert => {
                        array::array_insert(self, process, &code, instruction);
                    }
                    InstructionType::ArrayAt => {
                        array::array_at(self, process, &code, instruction);
                    }
                    InstructionType::ArrayRemove => {
                        array::array_remove(self, process, &code, instruction);
                    }
                    InstructionType::ArrayLength => {
                        array::array_length(self, process, &code, instruction);
                    }
                    InstructionType::ArrayClear => {
                        array::array_clear(self, process, &code, instruction);
                    }
                    InstructionType::StringToLower => {
                        string::string_to_lower(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::StringToUpper => {
                        string::string_to_upper(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::StringEquals => {
                        string::string_equals(self, process, &code, instruction);
                    }
                    InstructionType::StringToBytes => {
                        string::string_to_bytes(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::StringFromBytes => {
                        string::string_from_bytes(self,
                                                  process,
                                                  &code,
                                                  instruction);
                    }
                    InstructionType::StringLength => {
                        string::string_length(self, process, &code, instruction);
                    }
                    InstructionType::StringSize => {
                        string::string_size(self, process, &code, instruction);
                    }
                    InstructionType::StdoutWrite => {
                        stdout::stdout_write(self, process, &code, instruction);
                    }
                    InstructionType::StderrWrite => {
                        stderr::stderr_write(self, process, &code, instruction);
                    }
                    InstructionType::StdinRead => {
                        stdin::stdin_read(self, process, &code, instruction);
                    }
                    InstructionType::StdinReadLine => {
                        stdin::stdin_read_line(self, process, &code, instruction);
                    }
                    InstructionType::FileOpen => {
                        file::file_open(self, process, &code, instruction);
                    }
                    InstructionType::FileWrite => {
                        file::file_write(self, process, &code, instruction);
                    }
                    InstructionType::FileRead => {
                        file::file_read(self, process, &code, instruction);
                    }
                    InstructionType::FileReadLine => {
                        file::file_read_line(self, process, &code, instruction);
                    }
                    InstructionType::FileFlush => {
                        file::file_flush(self, process, &code, instruction);
                    }
                    InstructionType::FileSize => {
                        file::file_size(self, process, &code, instruction);
                    }
                    InstructionType::FileSeek => {
                        file::file_seek(self, process, &code, instruction);
                    }
                    InstructionType::ParseFile => {
                        code_execution::parse_file(self,
                                                   process,
                                                   &code,
                                                   instruction);
                    }
                    InstructionType::FileParsed => {
                        code_execution::file_parsed(self,
                                                    process,
                                                    &code,
                                                    instruction);
                    }
                    InstructionType::GetBindingPrototype => {
                        prototype::get_binding_prototype(self,
                                                         process,
                                                         &code,
                                                         instruction);
                    }
                    InstructionType::GetBinding => {
                        binding::get_binding(self, process, &code, instruction);
                    }
                    InstructionType::SetConstant => {
                        constant::set_const(self, process, &code, instruction);
                    }
                    InstructionType::GetConstant => {
                        constant::get_const(self, process, &code, instruction);
                    }
                    InstructionType::SetAttribute => {
                        object::set_attr(self, process, &code, instruction);
                    }
                    InstructionType::GetAttribute => {
                        object::get_attr(self, process, &code, instruction);
                    }
                    InstructionType::SetPrototype => {
                        prototype::set_prototype(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::GetPrototype => {
                        prototype::get_prototype(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::LocalExists => {
                        local_variable::local_exists(self,
                                                     process,
                                                     &code,
                                                     instruction);
                    }
                    InstructionType::RespondsTo => {
                        method::responds_to(self, process, &code, instruction);
                    }
                    InstructionType::SpawnProcess => {
                        process::spawn_process(self, process, &code, instruction);
                    }
                    InstructionType::SendProcessMessage => {
                        process::send_process_message(self,
                                                      process,
                                                      &code,
                                                      instruction);
                    }
                    InstructionType::ReceiveProcessMessage => {
                        let suspend =
                            process::receive_process_message(self,
                                                             process,
                                                             &code,
                                                             instruction);

                        if suspend {
                            suspend_retry!(self, context, process, index);
                        }
                    }
                    InstructionType::GetCurrentPid => {
                        process::get_current_pid(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::SetParentLocal => {
                        local_variable::set_parent_local(self,
                                                         process,
                                                         &code,
                                                         instruction);
                    }
                    InstructionType::GetParentLocal => {
                        local_variable::get_parent_local(self,
                                                         process,
                                                         &code,
                                                         instruction);
                    }
                    InstructionType::ErrorToInteger => {
                        error::error_to_integer(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::FileReadExact => {
                        file::file_read_exact(self, process, &code, instruction);
                    }
                    InstructionType::StdinReadExact => {
                        stdin::stdin_read_exact(self,
                                                process,
                                                &code,
                                                instruction);
                    }
                    InstructionType::ObjectEquals => {
                        object::object_equals(self, process, &code, instruction);
                    }
                    InstructionType::GetToplevel => {
                        object::get_toplevel(self, process, &code, instruction);
                    }
                    InstructionType::GetNilPrototype => {
                        prototype::get_nil_prototype(self,
                                                     process,
                                                     &code,
                                                     instruction);
                    }
                    InstructionType::GetNil => {
                        nil::get_nil(self, process, &code, instruction);
                    }
                    InstructionType::LookupMethod => {
                        method::lookup_method(self, process, &code, instruction);
                    }
                    InstructionType::AttrExists => {
                        object::attr_exists(self, process, &code, instruction);
                    }
                    InstructionType::ConstExists => {
                        constant::const_exists(self, process, &code, instruction);
                    }
                    InstructionType::RemoveMethod => {
                        method::remove_method(self, process, &code, instruction);
                    }
                    InstructionType::RemoveAttribute => {
                        object::remove_attribute(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::GetMethods => {
                        method::get_methods(self, process, &code, instruction);
                    }
                    InstructionType::GetMethodNames => {
                        method::get_method_names(self,
                                                 process,
                                                 &code,
                                                 instruction);
                    }
                    InstructionType::GetAttributes => {
                        object::get_attributes(self, process, &code, instruction);
                    }
                    InstructionType::GetAttributeNames => {
                        object::get_attribute_names(self,
                                                    process,
                                                    &code,
                                                    instruction);
                    }
                    InstructionType::MonotonicTimeNanoseconds => {
                        time::monotonic_time_nanoseconds(self,
                                                         process,
                                                         &code,
                                                         instruction);
                    }
                    InstructionType::MonotonicTimeMilliseconds => {
                        time::monotonic_time_milliseconds(self,
                                                          process,
                                                          &code,
                                                          instruction);
                    }
                    InstructionType::RunBlock => {
                        code_execution::run_block(self,
                                                  process,
                                                  &code,
                                                  instruction);

                        enter_context!(context, index, 'exec_loop);
                    }
                    InstructionType::RunBlockWithRest => {
                        code_execution::run_block_with_rest(self,
                                                            process,
                                                            &code,
                                                            instruction);

                        enter_context!(context, index, 'exec_loop);
                    }
                };
            } // while

            // Make sure that we update the stored instruction index in case we
            // need to suspend for garbage collection.
            //
            // This is important as the collector may reschedule an already
            // finished process. In that case we don't want to re-run any
            // previously executed instructions.
            context.instruction_index = index;

            // Once we're at the top-level _and_ we have no more instructions to
            // process we'll bail out of the main execution loop.
            if process.at_top_level() {
                break;
            }

            // We're not yet at the top level but we did finish running an
            // entire execution context.
            process.pop_context();

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

    /// Prints a VM backtrace of a given process with a message.
    fn error(&self, process: &RcProcess, error: String) {
        let mut message = format!("A fatal VM error occurred in process {}:",
                                  process.pid);

        message.push_str(&format!("\n\n{}\n\nCall stack:\n\n", error));

        for context in process.context().contexts() {
            message.push_str(&format!("{} line {} in {}\n",
                                      context.file(),
                                      context.line,
                                      context.name()));
        }

        self.terminate();
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

    /// Spawns a new process.
    pub fn spawn_process(&self,
                         process: &RcProcess,
                         pool_id: usize,
                         code: RcCompiledCode,
                         register: usize)
                         -> Result<(), String> {
        let new_proc = self.allocate_process(pool_id, code)?;
        let new_pid = new_proc.pid;

        self.state.process_pools.schedule(new_proc);

        process.set_register(register, ObjectPointer::integer(new_pid as i64));

        Ok(())
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    ///
    /// Returns true if a process should be suspended for garbage collection.
    fn gc_safepoint(&self, process: &RcProcess) -> bool {
        let request_opt = if process.should_collect_young_generation() {
            Some(GcRequest::heap(self.state.clone(), process.clone()))
        } else if process.should_collect_mailbox() {
            Some(GcRequest::mailbox(self.state.clone(), process.clone()))
        } else {
            None
        };

        if let Some(request) = request_opt {
            process.suspend_for_gc();
            self.state.gc_pool.schedule(request);

            true
        } else {
            false
        }
    }

    /// Reschedules a process.
    fn reschedule(&self, process: RcProcess) {
        self.state.process_pools.schedule(process);
    }
}
