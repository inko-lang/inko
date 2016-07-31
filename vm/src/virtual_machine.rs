//! Virtual Machine for running instructions
use std::collections::HashSet;
use std::io::{self, Write, Read, Seek, SeekFrom};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::thread;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;

use immix::copy_object::CopyObject;
use immix::global_allocator::{GlobalAllocator, RcGlobalAllocator};
use immix::permanent_allocator::PermanentAllocator;

use binding::RcBinding;
use bytecode_parser;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use config::Config;
use errors;
use gc::thread::Thread as GcThread;
use gc::request::{Request as GcRequest, Generation as GcGeneration};
use instruction::{InstructionType, Instruction};
use object_pointer::ObjectPointer;
use object_value;
use virtual_machine_error::VirtualMachineError;
use virtual_machine_result::*;
use process;
use process::{RcProcess, Process};
use process_list::ProcessList;
use execution_context::ExecutionContext;
use thread::{RcThread, JoinHandle as ThreadJoinHandle};
use thread_list::ThreadList;
use queue::Queue;

pub type RcVirtualMachineState = Arc<VirtualMachineState>;

pub struct VirtualMachineState {
    pub gc_requests: Queue<GcRequest>,
    config: Config,
    executed_files: RwLock<HashSet<String>>,
    threads: RwLock<ThreadList>,
    processes: RwLock<ProcessList>,
    exit_status: RwLock<Result<(), ()>>,

    permanent_allocator: RwLock<PermanentAllocator>,
    global_allocator: RcGlobalAllocator,
    top_level: ObjectPointer,
    integer_prototype: ObjectPointer,
    float_prototype: ObjectPointer,
    string_prototype: ObjectPointer,
    array_prototype: ObjectPointer,
    true_prototype: ObjectPointer,
    false_prototype: ObjectPointer,
    file_prototype: ObjectPointer,
    method_prototype: ObjectPointer,
    compiled_code_prototype: ObjectPointer,
    binding_prototype: ObjectPointer,
    true_object: ObjectPointer,
    false_object: ObjectPointer,
}

pub struct VirtualMachine {
    pub state: RcVirtualMachineState,
}

impl VirtualMachineState {
    pub fn new(config: Config) -> RcVirtualMachineState {
        let global_alloc = GlobalAllocator::new();
        let mut perm_alloc = PermanentAllocator::new(global_alloc.clone());

        let top_level = perm_alloc.allocate_empty();
        let integer_proto = perm_alloc.allocate_empty();
        let float_proto = perm_alloc.allocate_empty();
        let string_proto = perm_alloc.allocate_empty();
        let array_proto = perm_alloc.allocate_empty();
        let true_proto = perm_alloc.allocate_empty();
        let false_proto = perm_alloc.allocate_empty();
        let file_proto = perm_alloc.allocate_empty();
        let method_proto = perm_alloc.allocate_empty();
        let cc_proto = perm_alloc.allocate_empty();
        let binding_proto = perm_alloc.allocate_empty();

        let true_obj = perm_alloc.allocate_empty();
        let false_obj = perm_alloc.allocate_empty();

        {
            true_obj.get_mut().set_prototype(true_proto.clone());
            false_obj.get_mut().set_prototype(false_proto.clone());
        }

        let state = VirtualMachineState {
            config: config,
            executed_files: RwLock::new(HashSet::new()),
            threads: RwLock::new(ThreadList::new()),
            processes: RwLock::new(ProcessList::new()),
            gc_requests: Queue::new(),
            exit_status: RwLock::new(Ok(())),
            permanent_allocator: RwLock::new(perm_alloc),
            global_allocator: global_alloc,
            top_level: top_level,
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            true_prototype: true_proto,
            false_prototype: false_proto,
            file_prototype: file_proto,
            method_prototype: method_proto,
            compiled_code_prototype: cc_proto,
            binding_prototype: binding_proto,
            true_object: true_obj,
            false_object: false_obj,
        };

        Arc::new(state)
    }
}

impl VirtualMachine {
    pub fn new(state: RcVirtualMachineState) -> VirtualMachine {
        VirtualMachine { state: state }
    }

    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Starts the main thread
    ///
    /// This requires a RcCompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    pub fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        for _ in 0..self.config().process_threads {
            self.start_thread();
        }

        for _ in 0..self.config().gc_threads {
            self.start_gc_thread()
        }

        let thread = self.allocate_main_thread();
        let (_, process) =
            self.allocate_process(code, self.state.top_level.clone());

        thread.schedule(process);

        self.run_thread(thread);

        *read_lock!(self.state.exit_status)
    }

    fn allocate_thread(&self, handle: Option<ThreadJoinHandle>) -> RcThread {
        write_lock!(self.state.threads).add(handle)
    }

    fn allocate_main_thread(&self) -> RcThread {
        write_lock!(self.state.threads).add_main_thread()
    }

    /// Allocates a new process and returns the PID and Process structure.
    fn allocate_process(&self,
                        code: RcCompiledCode,
                        self_obj: ObjectPointer)
                        -> (usize, RcProcess) {
        let mut processes = write_lock!(self.state.processes);
        let pid = processes.reserve_pid();
        let process = Process::from_code(pid,
                                         code,
                                         self_obj,
                                         self.state.global_allocator.clone());

        processes.add(pid, process.clone());

        (pid, process)
    }

    fn allocate_method(&self,
                       process: &RcProcess,
                       receiver: &ObjectPointer,
                       code: RcCompiledCode)
                       -> ObjectPointer {
        let value = object_value::compiled_code(code);
        let proto = self.state.method_prototype.clone();

        if receiver.is_permanent() {
            write_lock!(self.state.permanent_allocator)
                .allocate_with_prototype(value, proto)
        } else {
            process.allocate(value, proto)
        }
    }

    /// Runs a single Process.
    fn run(&self, process: RcProcess) -> EmptyResult {
        let mut reductions = self.config().reductions;
        let mut suspend_retry = false;

        process.running();

        'exec_loop: loop {
            self.gc_safepoint(process.clone());

            let mut goto_index = None;
            let code = process.compiled_code();
            let mut index = process.instruction_index();
            let count = code.instructions.len();

            while index < count {
                let ref instruction = code.instructions[index];

                index += 1;

                match instruction.instruction_type {
                    InstructionType::SetInteger => {
                        run!(self, ins_set_integer, process, code, instruction);
                    }
                    InstructionType::SetFloat => {
                        run!(self, ins_set_float, process, code, instruction);
                    }
                    InstructionType::SetString => {
                        run!(self, ins_set_string, process, code, instruction);
                    }
                    InstructionType::SetObject => {
                        run!(self, ins_set_object, process, code, instruction);
                    }
                    InstructionType::SetPrototype => {
                        run!(self, ins_set_prototype, process, code, instruction);
                    }
                    InstructionType::GetPrototype => {
                        run!(self, ins_get_prototype, process, code, instruction);
                    }
                    InstructionType::SetArray => {
                        run!(self, ins_set_array, process, code, instruction);
                    }
                    InstructionType::GetIntegerPrototype => {
                        run!(self,
                             ins_get_integer_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetFloatPrototype => {
                        run!(self,
                             ins_get_float_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetStringPrototype => {
                        run!(self,
                             ins_get_string_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetArrayPrototype => {
                        run!(self,
                             ins_get_array_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetTruePrototype => {
                        run!(self,
                             ins_get_true_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetFalsePrototype => {
                        run!(self,
                             ins_get_false_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetMethodPrototype => {
                        run!(self,
                             ins_get_method_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetCompiledCodePrototype => {
                        run!(self,
                             ins_get_compiled_code_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetBindingPrototype => {
                        run!(self,
                             ins_get_binding_prototype,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetTrue => {
                        run!(self, ins_get_true, process, code, instruction);
                    }
                    InstructionType::GetFalse => {
                        run!(self, ins_get_false, process, code, instruction);
                    }
                    InstructionType::GetBinding => {
                        run!(self, ins_get_binding, process, code, instruction);
                    }
                    InstructionType::GetBindingOfCaller => {
                        run!(self,
                             ins_get_binding_of_caller,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetLocal => {
                        run!(self, ins_set_local, process, code, instruction);
                    }
                    InstructionType::GetLocal => {
                        run!(self, ins_get_local, process, code, instruction);
                    }
                    InstructionType::LocalExists => {
                        run!(self, ins_local_exists, process, code, instruction);
                    }
                    InstructionType::SetLiteralConst => {
                        run!(self,
                             ins_set_literal_const,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetConst => {
                        run!(self, ins_set_const, process, code, instruction);
                    }
                    InstructionType::GetLiteralConst => {
                        run!(self,
                             ins_get_literal_const,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetConst => {
                        run!(self, ins_get_const, process, code, instruction);
                    }
                    InstructionType::LiteralConstExists => {
                        run!(self,
                             ins_literal_const_exists,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetLiteralAttr => {
                        run!(self,
                             ins_set_literal_attr,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetAttr => {
                        run!(self, ins_set_attr, process, code, instruction);
                    }
                    InstructionType::GetLiteralAttr => {
                        run!(self,
                             ins_get_literal_attr,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetAttr => {
                        run!(self, ins_get_attr, process, code, instruction);
                    }
                    InstructionType::LiteralAttrExists => {
                        run!(self,
                             ins_literal_attr_exists,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetCompiledCode => {
                        run!(self,
                             ins_set_compiled_code,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SendLiteral => {
                        process.set_instruction_index(index);

                        run!(self, ins_send_literal, process, code, instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::Send => {
                        process.set_instruction_index(index);

                        run!(self, ins_send, process, code, instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::LiteralRespondsTo => {
                        run!(self,
                             ins_literal_responds_to,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::RespondsTo => {
                        run!(self, ins_responds_to, process, code, instruction);
                    }
                    InstructionType::Return => {
                        run!(self, ins_return, process, code, instruction);

                        break;
                    }
                    InstructionType::GotoIfFalse => {
                        goto_index = run!(self,
                                          ins_goto_if_false,
                                          process,
                                          code,
                                          instruction);
                    }
                    InstructionType::GotoIfTrue => {
                        goto_index = run!(self,
                                          ins_goto_if_true,
                                          process,
                                          code,
                                          instruction);
                    }
                    InstructionType::Goto => {
                        index = run!(self, ins_goto, process, code, instruction)
                            .unwrap();
                    }
                    InstructionType::DefMethod => {
                        run!(self, ins_def_method, process, code, instruction);
                    }
                    InstructionType::DefLiteralMethod => {
                        run!(self,
                             ins_def_literal_method,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::RunCode => {
                        process.set_instruction_index(index);

                        run!(self, ins_run_code, process, code, instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::RunLiteralCode => {
                        process.set_instruction_index(index);

                        run!(self,
                             ins_run_literal_code,
                             process,
                             code,
                             instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::GetToplevel => {
                        run!(self, ins_get_toplevel, process, code, instruction);
                    }
                    InstructionType::GetSelf => {
                        run!(self, ins_get_self, process, code, instruction);
                    }
                    InstructionType::IsError => {
                        run!(self, ins_is_error, process, code, instruction);
                    }
                    InstructionType::ErrorToString => {
                        run!(self,
                             ins_error_to_integer,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerAdd => {
                        run!(self, ins_integer_add, process, code, instruction);
                    }
                    InstructionType::IntegerDiv => {
                        run!(self, ins_integer_div, process, code, instruction);
                    }
                    InstructionType::IntegerMul => {
                        run!(self, ins_integer_mul, process, code, instruction);
                    }
                    InstructionType::IntegerSub => {
                        run!(self, ins_integer_sub, process, code, instruction);
                    }
                    InstructionType::IntegerMod => {
                        run!(self, ins_integer_mod, process, code, instruction);
                    }
                    InstructionType::IntegerToFloat => {
                        run!(self,
                             ins_integer_to_float,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerToString => {
                        run!(self,
                             ins_integer_to_string,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerBitwiseAnd => {
                        run!(self,
                             ins_integer_bitwise_and,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerBitwiseOr => {
                        run!(self,
                             ins_integer_bitwise_or,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerBitwiseXor => {
                        run!(self,
                             ins_integer_bitwise_xor,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerShiftLeft => {
                        run!(self,
                             ins_integer_shift_left,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerShiftRight => {
                        run!(self,
                             ins_integer_shift_right,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerSmaller => {
                        run!(self,
                             ins_integer_smaller,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerGreater => {
                        run!(self,
                             ins_integer_greater,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::IntegerEquals => {
                        run!(self,
                             ins_integer_equals,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SpawnLiteralProcess => {
                        run!(self,
                             ins_spawn_literal_process,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::FloatAdd => {
                        run!(self, ins_float_add, process, code, instruction);
                    }
                    InstructionType::FloatMul => {
                        run!(self, ins_float_mul, process, code, instruction);
                    }
                    InstructionType::FloatDiv => {
                        run!(self, ins_float_div, process, code, instruction);
                    }
                    InstructionType::FloatSub => {
                        run!(self, ins_float_sub, process, code, instruction);
                    }
                    InstructionType::FloatMod => {
                        run!(self, ins_float_mod, process, code, instruction);
                    }
                    InstructionType::FloatToInteger => {
                        run!(self,
                             ins_float_to_integer,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::FloatToString => {
                        run!(self,
                             ins_float_to_string,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::FloatSmaller => {
                        run!(self, ins_float_smaller, process, code, instruction);
                    }
                    InstructionType::FloatGreater => {
                        run!(self, ins_float_greater, process, code, instruction);
                    }
                    InstructionType::FloatEquals => {
                        run!(self, ins_float_equals, process, code, instruction);
                    }
                    InstructionType::ArrayInsert => {
                        run!(self, ins_array_insert, process, code, instruction);
                    }
                    InstructionType::ArrayAt => {
                        run!(self, ins_array_at, process, code, instruction);
                    }
                    InstructionType::ArrayRemove => {
                        run!(self, ins_array_remove, process, code, instruction);
                    }
                    InstructionType::ArrayLength => {
                        run!(self, ins_array_length, process, code, instruction);
                    }
                    InstructionType::ArrayClear => {
                        run!(self, ins_array_clear, process, code, instruction);
                    }
                    InstructionType::StringToLower => {
                        run!(self,
                             ins_string_to_lower,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::StringToUpper => {
                        run!(self,
                             ins_string_to_upper,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::StringEquals => {
                        run!(self, ins_string_equals, process, code, instruction);
                    }
                    InstructionType::StringToBytes => {
                        run!(self,
                             ins_string_to_bytes,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::StringFromBytes => {
                        run!(self,
                             ins_string_from_bytes,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::StringLength => {
                        run!(self, ins_string_length, process, code, instruction);
                    }
                    InstructionType::StringSize => {
                        run!(self, ins_string_size, process, code, instruction);
                    }
                    InstructionType::StdoutWrite => {
                        run!(self, ins_stdout_write, process, code, instruction);
                    }
                    InstructionType::StderrWrite => {
                        run!(self, ins_stderr_write, process, code, instruction);
                    }
                    InstructionType::StdinRead => {
                        run!(self, ins_stdin_read, process, code, instruction);
                    }
                    InstructionType::StdinReadLine => {
                        run!(self,
                             ins_stdin_read_line,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::FileOpen => {
                        run!(self, ins_file_open, process, code, instruction);
                    }
                    InstructionType::FileWrite => {
                        run!(self, ins_file_write, process, code, instruction);
                    }
                    InstructionType::FileRead => {
                        run!(self, ins_file_read, process, code, instruction);
                    }
                    InstructionType::FileReadLine => {
                        run!(self,
                             ins_file_read_line,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::FileFlush => {
                        run!(self, ins_file_flush, process, code, instruction);
                    }
                    InstructionType::FileSize => {
                        run!(self, ins_file_size, process, code, instruction);
                    }
                    InstructionType::FileSeek => {
                        run!(self, ins_file_seek, process, code, instruction);
                    }
                    InstructionType::RunLiteralFile => {
                        process.set_instruction_index(index);

                        run!(self,
                             ins_run_literal_file,
                             process,
                             code,
                             instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::RunFile => {
                        process.set_instruction_index(index);

                        run!(self, ins_run_file, process, code, instruction);

                        continue 'exec_loop;
                    }
                    InstructionType::GetCaller => {
                        run!(self, ins_get_caller, process, code, instruction);
                    }
                    InstructionType::SetOuterScope => {
                        run!(self,
                             ins_set_outer_scope,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SpawnProcess => {
                        run!(self, ins_spawn_process, process, code, instruction);
                    }
                    InstructionType::SendProcessMessage => {
                        run!(self,
                             ins_send_process_message,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::ReceiveProcessMessage => {
                        suspend_retry = run!(self,
                                             ins_receive_process_message,
                                             process,
                                             code,
                                             instruction);
                    }
                    InstructionType::GetCurrentPid => {
                        run!(self,
                             ins_get_current_pid,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::SetParentLocal => {
                        run!(self,
                             ins_set_parent_local,
                             process,
                             code,
                             instruction);
                    }
                    InstructionType::GetParentLocal => {
                        run!(self,
                             ins_get_parent_local,
                             process,
                             code,
                             instruction);
                    }
                };

                // Suspend at the current instruction and retry it once the
                // process is resumed again.
                if suspend_retry {
                    process.set_instruction_index(index - 1);
                    process.suspend();

                    return Ok(());
                }

                if let Some(idx) = goto_index {
                    index = idx;
                    goto_index = None;
                }
            } // while

            self.gc_safepoint(process.clone());

            // Once we're at the top-level _and_ we have no more instructions to
            // process we'll bail out of the main execution loop.
            if process.at_top_level() {
                break;
            }

            // We're not yet at the top level but we did finish running an
            // entire execution context.
            {
                process.pop_context();
                process.pop_call_frame();
            }

            // Reduce once we've exhausted all the instructions in a context.
            if reductions > 0 {
                reductions -= 1;
            } else {
                process.suspend();

                return Ok(());
            }
        }

        Ok(())
    }

    /// Sets an integer in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the integer in.
    /// 2. The index of the integer literals to use for the value.
    ///
    /// The integer literal is extracted from the given CompiledCode.
    fn ins_set_integer(&self,
                       process: RcProcess,
                       code: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let index = try_vm_error!(instruction.arg(1), instruction);
        let value = *try_vm_error!(code.integer(index), instruction);

        let obj = process.allocate(object_value::integer(value),
                                   self.state.integer_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets a float in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the float in.
    /// 2. The index of the float literals to use for the value.
    ///
    /// The float literal is extracted from the given CompiledCode.
    fn ins_set_float(&self,
                     process: RcProcess,
                     code: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let index = try_vm_error!(instruction.arg(1), instruction);
        let value = *try_vm_error!(code.float(index), instruction);

        let obj = process.allocate(object_value::float(value),
                                   self.state.float_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets a string in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the float in.
    /// 2. The index of the string literal to use for the value.
    ///
    /// The string literal is extracted from the given CompiledCode.
    fn ins_set_string(&self,
                      process: RcProcess,
                      code: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let index = try_vm_error!(instruction.arg(1), instruction);
        let value = try_vm_error!(code.string(index), instruction);

        let obj = process.allocate(object_value::string(value.clone()),
                                   self.state.string_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets an object in a register.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the object in.
    /// 2. A register containing a truthy/falsy object. When the register
    ///    contains a truthy object the new object will be a global object.
    /// 3. An optional register containing the prototype for the object.
    fn ins_set_object(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let is_permanent_ptr = instruction_object!(instruction, process, 1);
        let is_permanent = is_permanent_ptr != self.state.false_object.clone();

        let obj = if is_permanent {
            write_lock!(self.state.permanent_allocator).allocate_empty()
        } else {
            process.allocate_empty()
        };

        if let Ok(proto_index) = instruction.arg(2) {
            let mut proto = try_vm_error!(process.get_register(proto_index),
                                          instruction);

            if is_permanent && proto.is_local() {
                let (copy, _) = write_lock!(self.state.permanent_allocator)
                    .copy_object(proto);

                proto = copy;
            }

            obj.get_mut().set_prototype(proto);
        }

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets the prototype of an object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register containing the object for which to set the prototype.
    /// 2. The register containing the object to use as the prototype.
    fn ins_set_prototype(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        let source = instruction_object!(instruction, process, 0);
        let proto = instruction_object!(instruction, process, 1);

        source.get_mut().set_prototype(proto);

        Ok(())
    }

    /// Gets the prototype of an object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the prototype in.
    /// 2. The register containing the object to get the prototype from.
    fn ins_get_prototype(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);

        let source_obj = source.get();

        let proto = try_vm_error!(source_obj.prototype().ok_or_else(|| {
                                      format!("The object in register {} does \
                                               not have a prototype",
                                              instruction.arguments[1])
                                  }),
                                  instruction);

        process.set_register(register, proto);

        Ok(())
    }

    /// Sets an array in a register.
    ///
    /// This instruction requires at least one argument: the register to store
    /// the resulting array in. Any extra instruction arguments should point to
    /// registers containing objects to store in the array.
    fn ins_set_array(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let val_count = instruction.arguments.len() - 1;

        let values = try!(self.collect_arguments(process.clone(), instruction,
                                                 1, val_count));

        let obj = process.allocate(object_value::array(values),
                                   self.state.array_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Returns the prototype to use for integer objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_integer_prototype(&self,
                                 process: RcProcess,
                                 _: RcCompiledCode,
                                 instruction: &Instruction)
                                 -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.integer_prototype.clone());

        Ok(())
    }

    /// Returns the prototype to use for float objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_float_prototype(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.float_prototype.clone());

        Ok(())
    }

    /// Returns the prototype to use for string objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_string_prototype(&self,
                                process: RcProcess,
                                _: RcCompiledCode,
                                instruction: &Instruction)
                                -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.string_prototype.clone());

        Ok(())
    }

    /// Returns the prototype to use for array objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_array_prototype(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.array_prototype.clone());

        Ok(())
    }

    /// Gets the prototype to use for true objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_true_prototype(&self,
                              process: RcProcess,
                              _: RcCompiledCode,
                              instruction: &Instruction)
                              -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.true_prototype.clone());

        Ok(())
    }

    /// Gets the prototype to use for false objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_false_prototype(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.false_prototype.clone());

        Ok(())
    }

    /// Gets the prototype to use for method objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_method_prototype(&self,
                                process: RcProcess,
                                _: RcCompiledCode,
                                instruction: &Instruction)
                                -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.method_prototype.clone());

        Ok(())
    }

    /// Gets the prototype to use for Binding objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_binding_prototype(&self,
                                 process: RcProcess,
                                 _: RcCompiledCode,
                                 instruction: &Instruction)
                                 -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.binding_prototype.clone());

        Ok(())
    }

    /// Gets the prototype to use for compiled code objects.
    ///
    /// This instruction requires one argument: the register to store the
    /// prototype in.
    fn ins_get_compiled_code_prototype(&self,
                                       process: RcProcess,
                                       _: RcCompiledCode,
                                       instruction: &Instruction)
                                       -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process
            .set_register(register, self.state.compiled_code_prototype.clone());

        Ok(())
    }

    /// Sets a "true" value in a register.
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    fn ins_get_true(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.true_object.clone());

        Ok(())
    }

    /// Sets a "false" value in a register.
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    fn ins_get_false(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.false_object.clone());

        Ok(())
    }

    /// Gets the Binding of the current scope and sets it in a register
    ///
    /// This instruction requires only one argument: the register to store the
    /// object in.
    fn ins_get_binding(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let binding = process.binding();

        let obj = process.allocate(object_value::binding(binding),
                                   self.state.binding_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Gets the binding of a caller.
    ///
    /// If no binding could be found the current binding is returned instead.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the binding object in.
    /// 2. An integer indicating the amount of parents to walk upwards.
    fn ins_get_binding_of_caller(&self,
                                 process: RcProcess,
                                 _: RcCompiledCode,
                                 instruction: &Instruction)
                                 -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let depth = try_vm_error!(instruction.arg(1), instruction);
        let start_context = process.context();

        let binding = if let Some(context) = start_context.find_parent(depth) {
            context.binding()
        } else {
            start_context.binding()
        };

        let obj = process.allocate(object_value::binding(binding),
                                   self.state.binding_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets a local variable to a given register's value.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The local variable index to set.
    /// 2. The register containing the object to store in the variable.
    fn ins_set_local(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let local_index = try_vm_error!(instruction.arg(0), instruction);
        let object = instruction_object!(instruction, process, 1);

        process.set_local(local_index, object);

        Ok(())
    }

    /// Gets a local variable and stores it in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the local's value in.
    /// 2. The local variable index to get the value from.
    fn ins_get_local(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let local_index = try_vm_error!(instruction.arg(1), instruction);

        let object = try_vm_error!(process.get_local(local_index), instruction);

        process.set_register(register, object);

        Ok(())
    }

    /// Checks if a local variable exists.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in (true or false).
    /// 2. The local variable index to check.
    fn ins_local_exists(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let local_index = try_vm_error!(instruction.arg(1), instruction);

        let value = if process.local_exists(local_index) {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, value);

        Ok(())
    }

    /// Sets a local variable in one of the parent bindings.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The number of parent bindings to traverse in order to find the
    ///    binding to set the variable in.
    /// 2. The local variable index to set.
    /// 3. The register containing the value to set.
    fn ins_set_parent_local(&self,
                            process: RcProcess,
                            _: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let depth = try_vm_error!(instruction.arg(0), instruction);
        let index = try_vm_error!(instruction.arg(1), instruction);
        let value = instruction_object!(instruction, process, 2);

        if let Some(binding) = process.binding().find_parent(depth) {
            binding.set_local(index, value);
        } else {
            return_vm_error!(format!("No binding for depth {}", depth),
                             instruction.line);
        }

        Ok(())
    }

    /// Gets a local variable in one of the parent bindings.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The number of parent bindings to traverse in order to find the
    ///    binding to get the variable from.
    /// 2. The register to store the local variable in.
    /// 3. The local variable index to get.
    fn ins_get_parent_local(&self,
                            process: RcProcess,
                            _: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let depth = try_vm_error!(instruction.arg(0), instruction);
        let reg = try_vm_error!(instruction.arg(1), instruction);
        let index = try_vm_error!(instruction.arg(2), instruction);

        if let Some(binding) = process.binding().find_parent(depth) {
            let object = try_vm_error!(binding.get_local(index), instruction);

            process.set_register(reg, object);
        } else {
            return_vm_error!(format!("No binding for depth {}", depth),
                             instruction.line);
        }

        Ok(())
    }

    /// Sets a constant in a given object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register pointing to the object to store the constant in.
    /// 2. The string literal index to use for the name.
    /// 3. The register pointing to the object to store.
    fn ins_set_literal_const(&self,
                             process: RcProcess,
                             code: RcCompiledCode,
                             instruction: &Instruction)
                             -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_index = try_vm_error!(instruction.arg(1), instruction);
        let source_ptr = instruction_object!(instruction, process, 2);
        let name = try_vm_error!(code.string(name_index), instruction);

        let source = copy_if_permanent!(self.state.permanent_allocator,
                                        source_ptr,
                                        target_ptr);

        target_ptr.get_mut().add_constant(name.clone(), source);

        Ok(())
    }

    /// Sets a constant using a runtime allocated String.
    ///
    /// This instruction takes the same arguments as the "set_const" instruction
    /// except the 2nd argument should point to a register containing a String
    /// to use for the name.
    fn ins_set_const(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name = instruction_object!(instruction, process, 1);
        let source_ptr = instruction_object!(instruction, process, 2);

        let name_obj = name.get();

        ensure_strings!(instruction, name_obj);

        let name_str = name_obj.value.as_string().clone();

        let source = copy_if_permanent!(self.state.permanent_allocator,
                                        source_ptr,
                                        target_ptr);

        let target = target_ptr.get_mut();

        target.add_constant(name_str, source);

        Ok(())
    }

    /// Looks up a constant and stores it in a register.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the constant in.
    /// 2. The register pointing to an object in which to look for the
    ///    constant.
    /// 3. The string literal index containing the name of the constant.
    fn ins_get_literal_const(&self,
                             process: RcProcess,
                             code: RcCompiledCode,
                             instruction: &Instruction)
                             -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let src = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let name = try_vm_error!(code.string(name_index), instruction);

        let object = try_vm_error!(src.get()
                              .lookup_constant(name)
                              .ok_or_else(|| {
                                  constant_error!(instruction.arguments[1],
                                                  name)
                              }),
                          instruction);

        process.set_register(register, object);

        Ok(())
    }

    /// Looks up a constant using a runtime allocated string.
    ///
    /// This instruction requires the same arguments as the "get_literal_const"
    /// instruction except the last argument should point to a register
    /// containing a String to use for the name.
    fn ins_get_const(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let src = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_obj = name.get();

        ensure_strings!(instruction, name_obj);

        let name_str = name_obj.value.as_string();

        let object = try_vm_error!(src.get()
                              .lookup_constant(name_str)
                              .ok_or_else(|| {
                                  constant_error!(instruction.arguments[1],
                                                  name_str)
                              }),
                          instruction);

        process.set_register(register, object);

        Ok(())
    }

    /// Returns true if a constant exists, false otherwise.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the resulting boolean in.
    /// 2. The register containing the source object to check.
    /// 3. The string literal index to use as the constant name.
    fn ins_literal_const_exists(&self,
                                process: RcProcess,
                                code: RcCompiledCode,
                                instruction: &Instruction)
                                -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let name = try_vm_error!(code.string(name_index), instruction);

        let constant = source.get().lookup_constant(name);

        if constant.is_some() {
            process.set_register(register, self.state.true_object.clone());
        } else {
            process.set_register(register, self.state.false_object.clone());
        }

        Ok(())
    }

    /// Sets an attribute of an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register containing the object for which to set the
    ///    attribute.
    /// 2. The string literal index to use for the name.
    /// 3. The register containing the object to set as the attribute
    ///    value.
    fn ins_set_literal_attr(&self,
                            process: RcProcess,
                            code: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_index = try_vm_error!(instruction.arg(1), instruction);
        let value_ptr = instruction_object!(instruction, process, 2);

        let name = try_vm_error!(code.string(name_index), instruction);
        let value = copy_if_permanent!(self.state.permanent_allocator,
                                       value_ptr,
                                       target_ptr);

        target_ptr.get_mut().add_attribute(name.clone(), value);

        Ok(())
    }

    /// Sets an attribute of an object using a runtime allocated string.
    ///
    /// This instruction takes the same arguments as the "set_literal_attr"
    /// instruction except the 2nd argument should point to a register
    /// containing a String to use for the name.
    fn ins_set_attr(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_ptr = instruction_object!(instruction, process, 1);
        let value_ptr = instruction_object!(instruction, process, 2);

        let name_obj = name_ptr.get();

        ensure_strings!(instruction, name_obj);

        let name = name_obj.value.as_string();

        let value = copy_if_permanent!(self.state.permanent_allocator,
                                       value_ptr,
                                       target_ptr);

        target_ptr.get_mut().add_attribute(name.clone(), value);

        Ok(())
    }

    /// Gets an attribute from an object and stores it in a register.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the attribute's value in.
    /// 2. The register containing the object from which to retrieve the
    ///    attribute.
    /// 3. The string literal index to use for the name.
    fn ins_get_literal_attr(&self,
                            process: RcProcess,
                            code: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);

        let name = try_vm_error!(code.string(name_index), instruction);

        let attr = try_vm_error!(source.get()
                              .lookup_attribute(name)
                              .ok_or_else(|| {
                                  attribute_error!(instruction.arguments[1],
                                                   name)
                              }),
                          instruction);

        process.set_register(register, attr);

        Ok(())
    }

    /// Gets an object attribute using a runtime allocated string.
    ///
    /// This instruction takes the same arguments as the "get_literal_attr"
    /// instruction except the last argument should point to a register
    /// containing a String to use for the name.
    fn ins_get_attr(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_obj = name.get();

        ensure_strings!(instruction, name_obj);

        let name = name_obj.value.as_string();

        let attr = try_vm_error!(source.get()
                              .lookup_attribute(name)
                              .ok_or_else(|| {
                                  attribute_error!(instruction.arguments[1],
                                                   name)
                              }),
                          instruction);

        process.set_register(register, attr);

        Ok(())
    }

    /// Checks if an attribute exists in an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in (true or false).
    /// 2. The register containing the object to check.
    /// 3. The string literal index to use for the attribute name.
    fn ins_literal_attr_exists(&self,
                               process: RcProcess,
                               code: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source_ptr = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let name = try_vm_error!(code.string(name_index), instruction);

        let source = source_ptr.get();

        let obj = if source.has_attribute(name) {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, obj);

        Ok(())
    }

    /// Sets a CompiledCode object in a register.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the object in.
    /// 2. The index of the compiled code object to store.
    fn ins_set_compiled_code(&self,
                             process: RcProcess,
                             code: RcCompiledCode,
                             instruction: &Instruction)
                             -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let cc_index = try_vm_error!(instruction.arg(1), instruction);

        let cc = try_vm_error!(code.code_object(cc_index), instruction);

        let obj = process.allocate(object_value::compiled_code(cc),
                                   self.state
                                       .compiled_code_prototype
                                       .clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Sends a message using a string literal
    ///
    /// This instruction requires at least 5 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The index of the string literal to use for the method name.
    /// 4. A boolean (1 or 0) indicating if private methods can be called.
    /// 5. A boolean (1 or 0) to indicate if the last argument is a rest
    ///    argument. A rest argument will be unpacked into separate arguments.
    ///
    /// Any extra instruction arguments will be passed as arguments to the
    /// method.
    fn ins_send_literal(&self,
                        process: RcProcess,
                        code: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let name = try_vm_error!(code.string(name_index), instruction);

        self.send_message(name, process, instruction)
    }

    /// Sends a message using a runtime allocated string
    ///
    /// This instruction takes the same arguments as the "send_literal"
    /// instruction except instead of the 3rd argument pointing to a string
    /// literal it should point to a register containing a string.
    fn ins_send(&self,
                process: RcProcess,
                _: RcCompiledCode,
                instruction: &Instruction)
                -> EmptyResult {
        let string = instruction_object!(instruction, process, 2);
        let string_obj = string.get();

        ensure_strings!(instruction, string_obj);

        self.send_message(string_obj.value.as_string(), process, instruction)
    }

    /// Checks if an object responds to a message
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in (true or false)
    /// 2. The register containing the object to check
    /// 3. The string literal index to use as the method name
    fn ins_literal_responds_to(&self,
                               process: RcProcess,
                               code: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let name = try_vm_error!(code.string(name_index), instruction);

        let source_obj = source.get();

        let result = if source_obj.responds_to(name) {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, result);

        Ok(())
    }

    /// Checks if an object responds to a message using a runtime allocated
    /// string.
    ///
    /// This instruction requires the same arguments as the
    /// "literal_responds_to" instruction except the last argument should be a
    /// register containing a string.
    fn ins_responds_to(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_obj = name.get();
        let source_obj = source.get();

        ensure_strings!(instruction, name_obj);

        let result = if source_obj.responds_to(name_obj.value.as_string()) {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, result);

        Ok(())
    }

    /// Returns the value in the given register.
    ///
    /// As registers can be left empty this method returns an Option
    /// instead of returning an Object directly.
    ///
    /// This instruction takes a single argument: the register containing the
    /// value to return.
    fn ins_return(&self,
                  process: RcProcess,
                  _: RcCompiledCode,
                  instruction: &Instruction)
                  -> EmptyResult {
        let object = instruction_object!(instruction, process, 0);
        let current_context = process.context_mut();

        if let Some(register) = current_context.return_register {
            if let Some(parent_context) = current_context.parent_mut() {
                parent_context.set_register(register, object);
            }
        }

        Ok(())
    }

    /// Jumps to an instruction if a register is not set or set to false.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a register is not set.
    /// 2. The register to check.
    fn ins_goto_if_false(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> IntegerResult {
        let go_to = try_vm_error!(instruction.arg(0), instruction);
        let value_reg = try_vm_error!(instruction.arg(1), instruction);
        let value = process.get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if obj == self.state.false_object.clone() {
                    Some(go_to)
                } else {
                    None
                }
            }
            None => Some(go_to),
        };

        Ok(matched)
    }

    /// Jumps to an instruction if a register is set.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a register is set.
    /// 2. The register to check.
    fn ins_goto_if_true(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> IntegerResult {
        let go_to = try_vm_error!(instruction.arg(0), instruction);
        let value_reg = try_vm_error!(instruction.arg(1), instruction);
        let value = process.get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if obj == self.state.false_object.clone() {
                    None
                } else {
                    Some(go_to)
                }
            }
            None => None,
        };

        Ok(matched)
    }

    /// Jumps to a specific instruction.
    ///
    /// This instruction takes one argument: the instruction index to jump to.
    fn ins_goto(&self,
                _: RcProcess,
                _: RcCompiledCode,
                instruction: &Instruction)
                -> IntegerResult {
        let go_to = try_vm_error!(instruction.arg(0), instruction);

        Ok(Some(go_to))
    }

    /// Defines a method for an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the method object in.
    /// 2. The register pointing to a specific object to define the method
    ///    on.
    /// 3. The register containing a String to use as the method name.
    /// 4. The register containing the CompiledCode object to use for the
    ///    method.
    fn ins_def_method(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let name_ptr = instruction_object!(instruction, process, 2);
        let cc_ptr = instruction_object!(instruction, process, 3);

        let mut receiver = receiver_ptr.get_mut();

        let name_obj = name_ptr.get();

        ensure_strings!(instruction, name_obj);

        let cc_obj = cc_ptr.get();

        ensure_compiled_code!(instruction, cc_obj);

        let name = name_obj.value.as_string();
        let cc = cc_obj.value.as_compiled_code();

        let method = self.allocate_method(&process, &receiver_ptr, cc);

        receiver.add_method(name.clone(), method.clone());

        process.set_register(register, method);

        Ok(())
    }

    /// Defines a method for an object using literals.
    ///
    /// This instruction can be used to define a method when the name and the
    /// compiled code object are defined as literals. This instruction is
    /// primarily meant to define methods that are defined directly in the
    /// source code. Methods defined during runtime should be created using the
    /// `def_method` instruction instead.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the method object in.
    /// 2. The register pointing to the object to define the method on.
    /// 3. The string literal index to use for the method name.
    /// 4. The code object index to use for the method's CompiledCode object.
    fn ins_def_literal_method(&self,
                              process: RcProcess,
                              code: RcCompiledCode,
                              instruction: &Instruction)
                              -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let name_index = try_vm_error!(instruction.arg(2), instruction);
        let cc_index = try_vm_error!(instruction.arg(3), instruction);

        let name = try_vm_error!(code.string(name_index), instruction);
        let cc = try_vm_error!(code.code_object(cc_index), instruction);

        let mut receiver = receiver_ptr.get_mut();

        let method = self.allocate_method(&process, &receiver_ptr, cc);

        receiver.add_method(name.clone(), method.clone());

        process.set_register(register, method);

        Ok(())
    }

    /// Runs a runtime allocated CompiledCode.
    ///
    /// This instruction takes the following arguments:
    ///
    /// 1. The register to store the return value in.
    /// 2. The register containing the CompiledCode object to run.
    /// 3. The register containing an array of arguments to pass.
    /// 4. The Binding to use, if any. Omitting this argument results in a
    ///    Binding being created automatically.
    fn ins_run_code(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        process.advance_line(instruction.line);

        let register = try_vm_error!(instruction.arg(0), instruction);
        let cc_ptr = instruction_object!(instruction, process, 1);
        let args_ptr = instruction_object!(instruction, process, 2);

        let code_obj = {
            let cc_obj = cc_ptr.get();

            ensure_compiled_code!(instruction, cc_obj);

            cc_obj.value.as_compiled_code()
        };

        let args_obj = args_ptr.get();

        ensure_arrays!(instruction, args_obj);

        let arguments = args_obj.value.as_array();
        let arg_count = arguments.len();

        let binding_idx = 3 + arg_count;

        let binding = if instruction.arg(binding_idx).is_ok() {
            let obj_ptr = instruction_object!(instruction, process, binding_idx);
            let obj = obj_ptr.get();

            if !obj.value.is_binding() {
                return_vm_error!(format!("Argument {} is not a valid Binding",
                                         binding_idx),
                                 instruction.line);
            }

            Some(obj.value.as_binding())
        } else {
            None
        };

        self.schedule_code(process.clone(),
                           code_obj,
                           cc_ptr,
                           arguments,
                           binding,
                           register);

        process.pop_call_frame();

        Ok(())
    }

    /// Runs a CompiledCode literal.
    ///
    /// This instruction is meant to execute simple CompiledCode objects,
    /// usually the moment they're defined. For more complex use cases see the
    /// "run_code" instruction.
    ///
    /// This instruction takes the following arguments:
    ///
    /// 1. The register to store the return value in.
    /// 2. The index of the code object to run.
    /// 3. The register containing the object to use as "self" when running the
    ///    CompiledCode.
    fn ins_run_literal_code(&self,
                            process: RcProcess,
                            code: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        process.advance_line(instruction.line);

        let register = try_vm_error!(instruction.arg(0), instruction);
        let code_index = try_vm_error!(instruction.arg(1), instruction);
        let receiver = instruction_object!(instruction, process, 2);
        let code_obj = try_vm_error!(code.code_object(code_index), instruction);

        self.schedule_code(process.clone(),
                           code_obj,
                           receiver,
                           &Vec::new(),
                           None,
                           register);

        process.pop_call_frame();

        Ok(())
    }

    /// Sets the top-level object in a register.
    ///
    /// This instruction requires one argument: the register to store the object
    /// in.
    fn ins_get_toplevel(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        process.set_register(register, self.state.top_level.clone());

        Ok(())
    }

    /// Sets the object "self" refers to in a register.
    ///
    /// This instruction requires one argument: the register to store the object
    /// in.
    fn ins_get_self(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        let self_object = process.self_object();

        process.set_register(register, self_object);

        Ok(())
    }

    /// Checks if a given object is an error object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the boolean result in.
    /// 2. The register of the object to check.
    fn ins_is_error(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let obj_ptr = instruction_object!(instruction, process, 1);

        let obj = obj_ptr.get();

        let result = if obj.value.is_error() {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, result);

        Ok(())
    }

    /// Converts an error object to an integer.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the integer in.
    /// 2. The register containing the error.
    fn ins_error_to_integer(&self,
                            process: RcProcess,
                            _: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let error_ptr = instruction_object!(instruction, process, 1);
        let error = error_ptr.get();

        let proto = self.state.integer_prototype.clone();
        let integer = error.value.as_error() as i64;

        let result = process.allocate(object_value::integer(integer), proto);

        process.set_register(register, result);

        Ok(())
    }

    /// Adds two integers
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
    fn ins_integer_add(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        integer_op!(self, process, instruction, +);

        Ok(())
    }

    /// Divides an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
    fn ins_integer_div(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        integer_op!(self, process, instruction, /);

        Ok(())
    }

    /// Multiplies an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
    fn ins_integer_mul(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        integer_op!(self, process, instruction, *);

        Ok(())
    }

    /// Subtracts an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
    fn ins_integer_sub(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        integer_op!(self, process, instruction, -);

        Ok(())
    }

    /// Gets the modulo of an integer
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the left-hand side object.
    /// 3. The register of the right-hand side object.
    fn ins_integer_mod(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        integer_op!(self, process, instruction, %);

        Ok(())
    }

    /// Converts an integer to a float
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to convert.
    fn ins_integer_to_float(&self,
                            process: RcProcess,
                            _: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let integer_ptr = instruction_object!(instruction, process, 1);
        let integer = integer_ptr.get();

        ensure_integers!(instruction, integer);

        let result = integer.value.as_integer() as f64;

        let obj = process.allocate(object_value::float(result),
                                   self.state.float_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Converts an integer to a string
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to convert.
    fn ins_integer_to_string(&self,
                             process: RcProcess,
                             _: RcCompiledCode,
                             instruction: &Instruction)
                             -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let integer_ptr = instruction_object!(instruction, process, 1);
        let integer = integer_ptr.get();

        ensure_integers!(instruction, integer);

        let result = integer.value.as_integer().to_string();

        let obj = process.allocate(object_value::string(result),
                                   self.state.string_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Performs an integer bitwise AND.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
    fn ins_integer_bitwise_and(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        integer_op!(self, process, instruction, &);

        Ok(())
    }

    /// Performs an integer bitwise OR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
    fn ins_integer_bitwise_or(&self,
                              process: RcProcess,
                              _: RcCompiledCode,
                              instruction: &Instruction)
                              -> EmptyResult {
        integer_op!(self, process, instruction, |);

        Ok(())
    }

    /// Performs an integer bitwise XOR.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
    fn ins_integer_bitwise_xor(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        integer_op!(self, process, instruction, ^);

        Ok(())
    }

    /// Shifts an integer to the left.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
    fn ins_integer_shift_left(&self,
                              process: RcProcess,
                              _: RcCompiledCode,
                              instruction: &Instruction)
                              -> EmptyResult {
        integer_op!(self, process, instruction, <<);

        Ok(())
    }

    /// Shifts an integer to the right.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the integer to operate on.
    /// 3. The register of the integer to use as the operand.
    fn ins_integer_shift_right(&self,
                               process: RcProcess,
                               _: RcCompiledCode,
                               instruction: &Instruction)
                               -> EmptyResult {
        integer_op!(self, process, instruction, >>);

        Ok(())
    }

    /// Checks if one integer is smaller than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_integer_smaller(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        integer_bool_op!(self, process, instruction, <);

        Ok(())
    }

    /// Checks if one integer is greater than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_integer_greater(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        integer_bool_op!(self, process, instruction, >);

        Ok(())
    }

    /// Checks if two integers are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the integer to compare.
    /// 3. The register containing the integer to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_integer_equals(&self,
                          process: RcProcess,
                          _: RcCompiledCode,
                          instruction: &Instruction)
                          -> EmptyResult {
        integer_bool_op!(self, process, instruction, ==);

        Ok(())
    }

    /// Runs a CompiledCode in a new process.
    ///
    /// This instruction takes 2 arguments:
    ///
    /// 1. The register to store the PID in.
    /// 2. A code objects index pointing to the CompiledCode object to run.
    fn ins_spawn_literal_process(&self,
                                 process: RcProcess,
                                 code: RcCompiledCode,
                                 instruction: &Instruction)
                                 -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let code_index = try_vm_error!(instruction.arg(1), instruction);
        let code_obj = try_vm_error!(code.code_object(code_index), instruction);

        self.spawn_process(process, code_obj, register);

        Ok(())
    }

    /// Runs a CompiledCode in a new process using a runtime allocated
    /// CompiledCode.
    ///
    /// This instruction takes the same arguments as the "spawn_literal_process"
    /// instruction except instead of a code object index the 2nd argument
    /// should point to a register containing a CompiledCode object.
    fn ins_spawn_process(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let code_ptr = instruction_object!(instruction, process, 1);
        let code = code_ptr.get();

        ensure_compiled_code!(instruction, code);

        let code_obj = code.value.as_compiled_code();

        self.spawn_process(process, code_obj, register);

        Ok(())
    }

    /// Sends a message to a process.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the message in.
    /// 2. The register containing the PID to send the message to.
    /// 3. The register containing the message (an object) to send to the
    ///    process.
    fn ins_send_process_message(&self,
                                process: RcProcess,
                                _: RcCompiledCode,
                                instruction: &Instruction)
                                -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let pid_ptr = instruction_object!(instruction, process, 1);
        let msg_ptr = instruction_object!(instruction, process, 2);

        let pid = {
            let pid_obj = pid_ptr.get();

            ensure_integers!(instruction, pid_obj);

            pid_obj.value.as_integer() as usize
        };

        if let Some(receiver) = read_lock!(self.state.processes).get(pid) {
            receiver.send_message(msg_ptr.clone());
        }

        process.set_register(register, msg_ptr);

        Ok(())
    }

    /// Receives a message for the current process.
    ///
    /// This instruction takes 1 argument: the register to store the resulting
    /// message in.
    ///
    /// If no messages are available this instruction will block until a message
    /// is available.
    fn ins_receive_process_message(&self,
                                   process: RcProcess,
                                   _: RcCompiledCode,
                                   instruction: &Instruction)
                                   -> BooleanResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let pid = process.pid;
        let source = read_lock!(self.state.processes).get(pid).unwrap();

        if let Some(msg_ptr) = source.receive_message() {
            process.set_register(register, msg_ptr);

            Ok(false)
        } else {
            Ok(true)
        }
    }

    /// Gets the PID of the currently running process.
    ///
    /// This instruction requires one argument: the register to store the PID
    /// in (as an integer).
    fn ins_get_current_pid(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let pid = process.pid;

        let pid_obj = process.allocate(object_value::integer(pid as i64),
                                       self.state.integer_prototype.clone());

        process.set_register(register, pid_obj);

        Ok(())
    }

    /// Adds two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to add.
    fn ins_float_add(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        float_op!(self, process, instruction, +);

        Ok(())
    }

    /// Multiplies two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to multiply with.
    fn ins_float_mul(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        float_op!(self, process, instruction, *);

        Ok(())
    }

    /// Divides two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to divide with.
    fn ins_float_div(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        float_op!(self, process, instruction, /);

        Ok(())
    }

    /// Subtracts two floats
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float to subtract.
    fn ins_float_sub(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        float_op!(self, process, instruction, -);

        Ok(())
    }

    /// Gets the modulo of a float
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the receiver.
    /// 3. The register of the float argument.
    fn ins_float_mod(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        float_op!(self, process, instruction, %);

        Ok(())
    }

    /// Converts a float to an integer
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the float to convert.
    fn ins_float_to_integer(&self,
                            process: RcProcess,
                            _: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let float_ptr = instruction_object!(instruction, process, 1);
        let float = float_ptr.get();

        ensure_floats!(instruction, float);

        let result = float.value.as_float() as i64;

        let obj = process.allocate(object_value::integer(result),
                                   self.state.integer_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Converts a float to a string
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the float to convert.
    fn ins_float_to_string(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let float_ptr = instruction_object!(instruction, process, 1);
        let float = float_ptr.get();

        ensure_floats!(instruction, float);

        let result = float.value.as_float().to_string();

        let obj = process.allocate(object_value::string(result),
                                   self.state.string_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Checks if one float is smaller than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_float_smaller(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        float_bool_op!(self, process, instruction, <);

        Ok(())
    }

    /// Checks if one float is greater than the other.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_float_greater(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        float_bool_op!(self, process, instruction, >);

        Ok(())
    }

    /// Checks if two floats are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the float to compare.
    /// 3. The register containing the float to compare with.
    ///
    /// The result of this instruction is either boolean true or false.
    fn ins_float_equals(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        float_bool_op!(self, process, instruction, ==);

        Ok(())
    }

    /// Inserts a value in an array.
    ///
    /// This instruction requires 4 arguments:
    ///
    /// 1. The register to store the result (the inserted value) in.
    /// 2. The register containing the array to insert into.
    /// 3. The register containing the index (as an integer) to insert at.
    /// 4. The register containing the value to insert.
    ///
    /// An error is returned when the index is greater than the array length. A
    /// negative index can be used to indicate a position from the end of the
    /// array.
    fn ins_array_insert(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);
        let value_ptr = instruction_object!(instruction, process, 3);

        let mut array = array_ptr.get_mut();

        ensure_arrays!(instruction, array);

        let index_obj = index_ptr.get();

        ensure_integers!(instruction, index_obj);

        let mut vector = array.value.as_array_mut();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(instruction, vector, index);

        let value =
            copy_if_permanent!(self.state.permanent_allocator, value_ptr, array_ptr);

        vector.insert(index, value.clone());

        process.set_register(register, value);

        Ok(())
    }

    /// Gets the value of an array index.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the value in.
    /// 2. The register containing the array.
    /// 3. The register containing the index.
    ///
    /// An error is returned when the index is greater than the array length. A
    /// negative index can be used to indicate a position from the end of the
    /// array.
    fn ins_array_at(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);
        let array = array_ptr.get();

        let index_obj = index_ptr.get();

        ensure_arrays!(instruction, array);
        ensure_integers!(instruction, index_obj);

        let vector = array.value.as_array();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(instruction, vector, index);

        let value = vector[index].clone();

        process.set_register(register, value);

        Ok(())
    }

    /// Removes a value from an array.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the removed value in.
    /// 2. The register containing the array to remove a value from.
    /// 3. The register containing the index.
    ///
    /// An error is returned when the index is greater than the array length. A
    /// negative index can be used to indicate a position from the end of the
    /// array.
    fn ins_array_remove(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);

        let mut array = array_ptr.get_mut();
        let index_obj = index_ptr.get();

        ensure_arrays!(instruction, array);
        ensure_integers!(instruction, index_obj);

        let mut vector = array.value.as_array_mut();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(instruction, vector, index);

        let value = vector.remove(index);

        process.set_register(register, value);

        Ok(())
    }

    /// Gets the amount of elements in an array.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the length in.
    /// 2. The register containing the array.
    fn ins_array_length(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let array_ptr = instruction_object!(instruction, process, 1);

        let array = array_ptr.get();

        ensure_arrays!(instruction, array);

        let vector = array.value.as_array();
        let length = vector.len() as i64;

        let obj = process.allocate(object_value::integer(length),
                                   self.state.integer_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Removes all elements from an array.
    ///
    /// This instruction requires 1 argument: the register of the array.
    fn ins_array_clear(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        let array_ptr = instruction_object!(instruction, process, 0);

        let mut array = array_ptr.get_mut();

        ensure_arrays!(instruction, array);

        let mut vector = array.value.as_array_mut();

        vector.clear();

        Ok(())
    }

    /// Returns the lowercase equivalent of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the new string in.
    /// 2. The register containing the input string.
    fn ins_string_to_lower(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source_ptr = instruction_object!(instruction, process, 1);
        let source = source_ptr.get();

        ensure_strings!(instruction, source);

        let lower = source.value.as_string().to_lowercase();

        let obj = process.allocate(object_value::string(lower),
                                   self.state.string_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Returns the uppercase equivalent of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the new string in.
    /// 2. The register containing the input string.
    fn ins_string_to_upper(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let source_ptr = instruction_object!(instruction, process, 1);
        let source = source_ptr.get();

        ensure_strings!(instruction, source);

        let upper = source.value.as_string().to_uppercase();

        let obj = process.allocate(object_value::string(upper),
                                   self.state.string_prototype.clone());

        process.set_register(register, obj);

        Ok(())
    }

    /// Checks if two strings are equal.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string to compare.
    /// 3. The register of the string to compare with.
    fn ins_string_equals(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver = receiver_ptr.get();
        let arg = arg_ptr.get();

        ensure_strings!(instruction, receiver, arg);

        let result = receiver.value.as_string() == arg.value.as_string();

        let boolean = if result {
            self.state.true_object.clone()
        } else {
            self.state.false_object.clone()
        };

        process.set_register(register, boolean);

        Ok(())
    }

    /// Returns an array containing the bytes of a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the string to get the bytes from.
    fn ins_string_to_bytes(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_strings!(instruction, arg);

        let int_proto = self.state.integer_prototype.clone();
        let array_proto = self.state.array_prototype.clone();

        let array = arg.value
            .as_string()
            .as_bytes()
            .iter()
            .map(|&b| {
                process
                    .allocate(object_value::integer(b as i64), int_proto.clone())
            })
            .collect::<Vec<_>>();

        let obj = process.allocate(object_value::array(array), array_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Creates a string from an array of bytes
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register containing the array of bytes.
    ///
    /// The result of this instruction is either a string based on the given
    /// bytes, or an error object.
    fn ins_string_from_bytes(&self,
                             process: RcProcess,
                             _: RcCompiledCode,
                             instruction: &Instruction)
                             -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_arrays!(instruction, arg);

        let string_proto = self.state.string_prototype.clone();
        let array = arg.value.as_array();

        for int_ptr in array.iter() {
            let int_obj = int_ptr.get();

            ensure_integers!(instruction, int_obj);
        }

        let bytes = arg.value
            .as_array()
            .iter()
            .map(|ref int_ptr| int_ptr.get().value.as_integer() as u8)
            .collect::<Vec<_>>();

        let string = try_error!(try_from_utf8!(bytes), process, register);

        let obj = process.allocate(object_value::string(string), string_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Returns the amount of characters in a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string.
    fn ins_string_length(&self,
                         process: RcProcess,
                         _: RcCompiledCode,
                         instruction: &Instruction)
                         -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_strings!(instruction, arg);

        let int_proto = self.state.integer_prototype.clone();
        let length = arg.value.as_string().chars().count() as i64;

        let obj = process.allocate(object_value::integer(length), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Returns the amount of bytes in a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. The register of the string.
    fn ins_string_size(&self,
                       process: RcProcess,
                       _: RcCompiledCode,
                       instruction: &Instruction)
                       -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_strings!(instruction, arg);

        let int_proto = self.state.integer_prototype.clone();
        let size = arg.value.as_string().len() as i64;

        let obj = process.allocate(object_value::integer(size), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Writes a string to STDOUT and returns the amount of written bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the string to write.
    ///
    /// The result of this instruction is either an integer indicating the
    /// amount of bytes written, or an error object.
    fn ins_stdout_write(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_strings!(instruction, arg);

        let int_proto = self.state.integer_prototype.clone();
        let mut stdout = io::stdout();

        let result = try_io!(stdout.write(arg.value.as_string().as_bytes()),
                             process, register);

        try_io!(stdout.flush(), process, register);

        let obj =
            process.allocate(object_value::integer(result as i64), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Writes a string to STDERR and returns the amount of written bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the string to write.
    ///
    /// The result of this instruction is either an integer indicating the
    /// amount of bytes written, or an error object.
    fn ins_stderr_write(&self,
                        process: RcProcess,
                        _: RcCompiledCode,
                        instruction: &Instruction)
                        -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg = arg_ptr.get();

        ensure_strings!(instruction, arg);

        let int_proto = self.state.integer_prototype.clone();
        let mut stderr = io::stderr();

        let result = try_io!(stderr.write(arg.value.as_string().as_bytes()),
                             process, register);

        try_io!(stderr.flush(), process, register);

        let obj =
            process.allocate(object_value::integer(result as i64), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Reads the given amount of bytes into a string.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the amount of bytes to read.
    ///
    /// The result of this instruction is either a string containing the data
    /// read, or an error object.
    fn ins_stdin_read(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let proto = self.state.string_prototype.clone();

        let mut buffer = file_reading_buffer!(instruction, process, 1);

        try_io!(io::stdin().read_to_string(&mut buffer), process, register);

        let obj = process.allocate(object_value::string(buffer), proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Reads an entire line from STDIN into a string.
    ///
    /// This instruction requires 1 argument: the register to store the
    /// resulting object in.
    ///
    /// The result of this instruction is either a string containing the read
    /// data, or an error object.
    fn ins_stdin_read_line(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let proto = self.state.string_prototype.clone();

        let mut buffer = String::new();

        try_io!(io::stdin().read_line(&mut buffer), process, register);

        let obj = process.allocate(object_value::string(buffer), proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Opens a file handle in a particular mode (read-only, write-only, etc).
    ///
    /// This instruction requires X arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The path to the file to open.
    /// 3. The register containing a string describing the mode to open the
    ///    file in.
    ///
    /// The result of this instruction is either a file object or an error
    /// object.
    ///
    /// The available file modes supported are the same as those supported by
    /// the `fopen()` system call, thus:
    ///
    /// * r: opens a file for reading only
    /// * r+: opens a file for reading and writing
    /// * w: opens a file for writing only, truncating it if it exists, creating
    ///   it otherwise
    /// * w+: opens a file for reading and writing, truncating it if it exists,
    ///   creating it if it doesn't exist
    /// * a: opens a file for appending, creating it if it doesn't exist
    /// * a+: opens a file for reading and appending, creating it if it doesn't
    ///   exist
    fn ins_file_open(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let path_ptr = instruction_object!(instruction, process, 1);
        let mode_ptr = instruction_object!(instruction, process, 2);

        let file_proto = self.state.file_prototype.clone();

        let path = path_ptr.get();
        let mode = mode_ptr.get();

        let path_string = path.value.as_string();
        let mode_string = mode.value.as_string().as_ref();
        let mut open_opts = OpenOptions::new();

        match mode_string {
            "r" => open_opts.read(true),
            "r+" => open_opts.read(true).write(true).truncate(true).create(true),
            "w" => open_opts.write(true).truncate(true).create(true),
            "w+" => open_opts.read(true).write(true).truncate(true).create(true),
            "a" => open_opts.append(true).create(true),
            "a+" => open_opts.read(true).append(true).create(true),
            _ => set_error!(errors::IO_INVALID_OPEN_MODE, process, register),
        };

        let file = try_io!(open_opts.open(path_string), process, register);

        let obj = process.allocate(object_value::file(file), file_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Writes a string to a file.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the amount of written bytes in.
    /// 2. The register containing the file object to write to.
    /// 3. The register containing the string to write.
    ///
    /// The result of this instruction is either the amount of written bytes or
    /// an error object.
    fn ins_file_write(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);
        let string_ptr = instruction_object!(instruction, process, 2);

        let mut file = file_ptr.get_mut();
        let string = string_ptr.get();

        ensure_files!(instruction, file);
        ensure_strings!(instruction, string);

        let int_proto = self.state.integer_prototype.clone();
        let mut file = file.value.as_file_mut();
        let bytes = string.value.as_string().as_bytes();

        let result = try_io!(file.write(bytes), process, register);

        let obj =
            process.allocate(object_value::integer(result as i64), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Reads a number of bytes from a file.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file to read from.
    /// 3. The register containing the amount of bytes to read, if left out
    ///    all data is read instead.
    ///
    /// The result of this instruction is either a string containing the data
    /// read, or an error object.
    fn ins_file_read(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);

        let mut file_obj = file_ptr.get_mut();

        ensure_files!(instruction, file_obj);

        let mut buffer = file_reading_buffer!(instruction, process, 2);
        let int_proto = self.state.integer_prototype.clone();
        let mut file = file_obj.value.as_file_mut();

        try_io!(file.read_to_string(&mut buffer), process, register);

        let obj = process.allocate(object_value::string(buffer), int_proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Reads an entire line from a file.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file to read from.
    ///
    /// The result of this instruction is either a string containing the read
    /// line, or an error object.
    fn ins_file_read_line(&self,
                          process: RcProcess,
                          _: RcCompiledCode,
                          instruction: &Instruction)
                          -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);

        let mut file_obj = file_ptr.get_mut();

        ensure_files!(instruction, file_obj);

        let proto = self.state.string_prototype.clone();
        let mut file = file_obj.value.as_file_mut();
        let mut bytes = Vec::new();

        for result in file.bytes() {
            let byte = try_io!(result, process, register);

            bytes.push(byte);

            if byte == 0xA {
                break;
            }
        }

        let string = try_error!(try_from_utf8!(bytes), process, register);

        let obj = process.allocate(object_value::string(string), proto);

        process.set_register(register, obj);

        Ok(())
    }

    /// Flushes a file.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the result in.
    /// 2. the register containing the file to flush.
    ///
    /// The resulting object is either boolean true (upon success), or an error
    /// object.
    fn ins_file_flush(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);

        let mut file_obj = file_ptr.get_mut();

        ensure_files!(instruction, file_obj);

        let mut file = file_obj.value.as_file_mut();

        try_io!(file.flush(), process, register);

        process.set_register(register, self.state.true_object.clone());

        Ok(())
    }

    /// Returns the size of a file in bytes.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the file.
    ///
    /// The resulting object is either an integer representing the amount of
    /// bytes, or an error object.
    fn ins_file_size(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);

        let file_obj = file_ptr.get();

        ensure_files!(instruction, file_obj);

        let file = file_obj.value.as_file();
        let meta = try_io!(file.metadata(), process, register);

        let size = meta.len() as i64;
        let proto = self.state.integer_prototype.clone();

        let result = process.allocate(object_value::integer(size), proto);

        process.set_register(register, result);

        Ok(())
    }

    /// Sets a file cursor to the given offset in bytes.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The register containing the input file.
    /// 3. The offset to seek to as an integer.
    ///
    /// The resulting object is either an integer representing the new cursor
    /// position, or an error object.
    fn ins_file_seek(&self,
                     process: RcProcess,
                     _: RcCompiledCode,
                     instruction: &Instruction)
                     -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let file_ptr = instruction_object!(instruction, process, 1);
        let offset_ptr = instruction_object!(instruction, process, 2);

        let mut file_obj = file_ptr.get_mut();
        let offset_obj = offset_ptr.get();

        ensure_files!(instruction, file_obj);
        ensure_integers!(instruction, offset_obj);

        let mut file = file_obj.value.as_file_mut();
        let offset = offset_obj.value.as_integer();

        ensure_positive_read_size!(instruction, offset);

        let seek_from = SeekFrom::Start(offset as u64);
        let new_offset = try_io!(file.seek(seek_from), process, register);

        let proto = self.state.integer_prototype.clone();

        let result =
            process.allocate(object_value::integer(new_offset as i64), proto);

        process.set_register(register, result);

        Ok(())
    }

    /// Parses and runs a given bytecode file using a string literal
    ///
    /// Files are executed only once. After a file has been executed any
    /// following calls are basically no-ops.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register to store the resulting object in.
    /// 2. The string literal index containing the file path of the bytecode
    ///    file.
    ///
    /// The result of this instruction is whatever the bytecode file returned.
    fn ins_run_literal_file(&self,
                            process: RcProcess,
                            code: RcCompiledCode,
                            instruction: &Instruction)
                            -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let index = try_vm_error!(instruction.arg(1), instruction);
        let path = try_vm_error!(code.string(index), instruction);

        self.run_file(path, process, instruction, register)
    }

    /// Parses and runs a given bytecode file using a runtime allocated string
    ///
    /// This instruction takes the same arguments as the "run_literal_file"
    /// instruction except instead of using a string literal it uses a register
    /// containing a runtime allocated string.
    fn ins_run_file(&self,
                    process: RcProcess,
                    _: RcCompiledCode,
                    instruction: &Instruction)
                    -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);
        let path_ptr = instruction_object!(instruction, process, 1);

        let path = path_ptr.get();

        ensure_strings!(instruction, path);

        self.run_file(path.value.as_string(), process, instruction, register)
    }

    /// Sets the caller of a method.
    ///
    /// This instruction requires one argument: the register to store the caller
    /// in. If no caller is present "self" is set in the register instead.
    fn ins_get_caller(&self,
                      process: RcProcess,
                      _: RcCompiledCode,
                      instruction: &Instruction)
                      -> EmptyResult {
        let register = try_vm_error!(instruction.arg(0), instruction);

        let caller = {
            let context = process.context();

            if let Some(parent) = context.parent() {
                parent.self_object()
            } else {
                context.self_object()
            }
        };

        process.set_register(register, caller);

        Ok(())
    }

    /// Sets the outer scope of an object
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register containing the object for which to set the outer scope.
    /// 2. The register containing the object to use as the outer scope.
    fn ins_set_outer_scope(&self,
                           process: RcProcess,
                           _: RcCompiledCode,
                           instruction: &Instruction)
                           -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let scope_ptr = instruction_object!(instruction, process, 1);

        let mut target = target_ptr.get_mut();

        let scope =
            copy_if_permanent!(self.state.permanent_allocator, scope_ptr, target_ptr);

        target.set_outer_scope(scope);

        Ok(())
    }

    /// Prints a VM backtrace of a given thread with a message.
    fn error(&self, process: RcProcess, error: VirtualMachineError) {
        let mut stderr = io::stderr();
        let ref frame = process.call_frame();
        let mut message =
            format!("Fatal error:\n\n{}\n\nStacktrace:\n\n", error.message);

        message.push_str(&format!("{} line {} in {}\n", frame.file(),
                                  error.line, frame.name()));

        *write_lock!(self.state.exit_status) = Err(());

        frame.each_frame(|frame| {
            message.push_str(&format!(
                "{} line {} in {}\n",
                frame.file(),
                frame.line,
                frame.name()
            ));
        });

        stderr.write(message.as_bytes()).unwrap();

        stderr.flush().unwrap();
    }

    /// Schedules the execution of a new CompiledCode.
    fn schedule_code(&self,
                     process: RcProcess,
                     code: RcCompiledCode,
                     self_obj: ObjectPointer,
                     args: &Vec<ObjectPointer>,
                     binding: Option<RcBinding>,
                     register: usize) {
        let context = if let Some(rc_bind) = binding {
            ExecutionContext::with_binding(rc_bind, code.clone(), Some(register))
        } else {
            ExecutionContext::with_object(self_obj, code.clone(), Some(register))
        };

        let frame = CallFrame::from_code(code);

        process.push_context(context);
        process.push_call_frame(frame);

        for (index, arg) in args.iter().enumerate() {
            process.set_local(index, arg.clone());
        }
    }

    /// Runs a bytecode file.
    fn run_file(&self,
                path_str: &String,
                process: RcProcess,
                instruction: &Instruction,
                register: usize)
                -> EmptyResult {
        process.advance_line(instruction.line);

        {
            let mut executed = write_lock!(self.state.executed_files);

            if executed.contains(path_str) {
                return Ok(());
            } else {
                executed.insert(path_str.clone());
            }
        }

        let mut input_path = PathBuf::from(path_str);

        if input_path.is_relative() {
            let mut found = false;

            for directory in self.config().directories.iter() {
                let full_path = directory.join(path_str);

                if full_path.exists() {
                    input_path = full_path;
                    found = true;

                    break;
                }
            }

            if !found {
                return_vm_error!(format!("No file found for {}", path_str),
                                 instruction.line);
            }
        }

        let input_path_str = input_path.to_str().unwrap();

        match bytecode_parser::parse_file(input_path_str) {
            Ok(body) => {
                let self_obj = self.state.top_level.clone();

                self.schedule_code(process.clone(),
                                   body,
                                   self_obj,
                                   &Vec::new(),
                                   None,
                                   register);

                process.pop_call_frame();

                Ok(())
            }
            Err(err) => {
                return_vm_error!(
                    format!("Failed to parse {}: {:?}", input_path_str, err),
                    instruction.line
                );
            }
        }
    }

    /// Sends a message to an object.
    fn send_message(&self,
                    name: &String,
                    process: RcProcess,
                    instruction: &Instruction)
                    -> EmptyResult {
        // Advance the line number so error messages contain the correct frame
        // pointing to the call site.
        process.advance_line(instruction.line);

        let register = try_vm_error!(instruction.arg(0), instruction);
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let allow_private = try_vm_error!(instruction.arg(3), instruction);
        let rest_arg = try_vm_error!(instruction.arg(4), instruction) == 1;

        let method_ptr = {
            let receiver_ptr = receiver_ptr.get();

            try_vm_error!(
                receiver_ptr.lookup_method(name).ok_or_else(|| {
                    format!(
                        "undefined method \"{}\" called on an object of type {}",
                        name,
                        receiver_ptr.value.type_name()
                    )
                }),
                instruction
            )
        };

        let method_obj = method_ptr.get();

        ensure_compiled_code!(instruction, method_obj);

        let method_code = method_obj.value.as_compiled_code();

        if method_code.is_private() && allow_private == 0 {
            return_vm_error!(format!("Private method \"{}\" called", name),
                             instruction.line);
        }

        // Argument handling
        let arg_count = instruction.arguments.len() - 5;
        let tot_args = method_code.arguments as usize;
        let req_args = method_code.required_arguments as usize;

        let mut arguments = try!(
            self.collect_arguments(process.clone(), instruction, 5, arg_count)
        );

        // Unpack the last argument if it's a rest argument
        if rest_arg {
            if let Some(last_arg) = arguments.pop() {
                let array = last_arg.get();

                ensure_arrays!(instruction, array);

                for value in array.value.as_array() {
                    arguments.push(value.clone());
                }
            }
        }

        // If the method defines a rest argument we'll pack any excessive
        // arguments into a single array.
        if method_code.rest_argument && arguments.len() > tot_args {
            let rest_count = arguments.len() - tot_args;
            let mut rest = Vec::new();

            for obj in arguments[arguments.len() - rest_count..].iter() {
                rest.push(obj.clone());
            }

            arguments.truncate(tot_args);

            let rest_array = process.allocate(object_value::array(rest),
                                              self.state.array_prototype.clone());

            arguments.push(rest_array);
        } else if method_code.rest_argument && arguments.len() == 0 {
            let rest_array = process.allocate(object_value::array(Vec::new()),
                                              self.state.array_prototype.clone());

            arguments.push(rest_array);
        }

        if arguments.len() > tot_args && !method_code.rest_argument {
            return_vm_error!(
                format!(
                    "{} accepts up to {} arguments, but {} arguments were given",
                    name,
                    method_code.arguments,
                    arguments.len()
                ),
                instruction.line
            );
        }

        if arguments.len() < req_args {
            return_vm_error!(
                format!(
                    "{} requires {} arguments, but {} arguments were given",
                    name,
                    method_code.required_arguments,
                    arguments.len()
                ),
                instruction.line
            );
        }

        self.schedule_code(process.clone(),
                           method_code,
                           receiver_ptr.clone(),
                           &arguments,
                           None,
                           register);

        process.pop_call_frame();

        Ok(())
    }

    /// Collects a set of arguments from an instruction.
    fn collect_arguments(&self,
                         process: RcProcess,
                         instruction: &Instruction,
                         offset: usize,
                         amount: usize)
                         -> ObjectVecResult {
        let mut args: Vec<ObjectPointer> = Vec::new();

        for index in offset..(offset + amount) {
            let arg_index = try_vm_error!(instruction.arg(index), instruction);

            let arg = try_vm_error!(
                process.get_register(arg_index),
                instruction
            );

            args.push(arg)
        }

        Ok(args)
    }

    /// Starts a new thread.
    fn start_thread(&self) -> RcThread {
        let state_clone = self.state.clone();

        let (sender, receiver) = channel();

        let handle = thread::spawn(move || {
            let thread = receiver.recv().unwrap();
            let vm = VirtualMachine::new(state_clone);

            vm.run_thread(thread);
        });

        let thread = self.allocate_thread(Some(handle));

        sender.send(thread.clone()).unwrap();

        thread
    }

    /// Starts a new GC thread
    fn start_gc_thread(&self) {
        let state_clone = self.state.clone();

        thread::spawn(move || {
            let mut gc_thread = GcThread::new(state_clone);

            gc_thread.run();
        });
    }

    /// Spawns a new process.
    fn spawn_process(&self,
                     process: RcProcess,
                     code: RcCompiledCode,
                     register: usize) {
        let (pid, new_proc) =
            self.allocate_process(code, self.state.top_level.clone());

        write_lock!(self.state.threads).schedule(new_proc);

        let pid_obj = process.allocate(object_value::integer(pid as i64),
                                       self.state.integer_prototype.clone());

        process.set_register(register, pid_obj);
    }

    /// Start a thread's execution loop.
    fn run_thread(&self, thread: RcThread) {
        while !thread.should_stop() {
            // Bail out if any of the other threads errored.
            if read_lock!(self.state.exit_status).is_err() {
                break;
            }

            // Terminate gracefully once the main thread has processed its
            // process queue.
            if thread.main_thread && thread.process_queue_empty() {
                write_lock!(self.state.threads).stop();
                break;
            }

            thread.wait_for_work();

            // A thread may be woken up (e.g. due to a VM error) without there
            // being work available.
            if thread.process_queue_empty() {
                break;
            }

            let process = thread.pop_process();

            match self.run(process.clone()) {
                Ok(_) => {
                    let reschedule = process.is_suspended();

                    // Process exhausted reductions, re-schedule it.
                    if reschedule {
                        thread.schedule(process);
                    } else {
                        process.finished();

                        write_lock!(self.state.processes).remove(process);
                    }
                }
                // TODO: process supervision
                Err(err) => {
                    self.error(process, err);

                    write_lock!(self.state.threads).stop();
                }
            }
        }
    }

    /// Checks if a garbage collection run should be scheduled for the given
    /// process.
    fn gc_safepoint(&self, process: RcProcess) {
        match *process.gc_state() {
            process::GcState::ScheduleEden => {
                process.gc_scheduled();

                let roots = process.roots();

                let request = GcRequest::new(GcGeneration::Eden, process, roots);

                self.state.gc_requests.push(request);
            }
            process::GcState::ScheduleYoung => {}
            process::GcState::ScheduleMature => {}
            _ => {}
        }
    }
}
