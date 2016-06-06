//! Virtual Machine for running instructions
extern crate num_cpus;

use std::collections::HashSet;
use std::io::{self, Write, Read, Seek, SeekFrom};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::thread;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;

use binding::RcBinding;
use bytecode_parser;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use errors;
use heap::Heap;
use instruction::{InstructionType, Instruction};
use object_pointer::ObjectPointer;
use object_value;
use virtual_machine_methods::VirtualMachineMethods;
use virtual_machine_result::*;
use process::{RcProcess, Process};
use process_list::ProcessList;
use thread::{RcThread, JoinHandle as ThreadJoinHandle};
use thread_list::ThreadList;

/// A reference counted VirtualMachine.
pub type RcVirtualMachine = Arc<VirtualMachine>;

/// Structure representing a single VM instance.
pub struct VirtualMachine {
    /// The directories to search for bytecode files.
    directories: RwLock<Vec<Box<Path>>>,
    executed_files: RwLock<HashSet<String>>,
    threads: RwLock<ThreadList>,
    processes: RwLock<ProcessList>,
    exit_status: RwLock<Result<(), ()>>,

    global_heap: RwLock<Heap>,
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
    false_object: ObjectPointer
}

impl VirtualMachine {
    pub fn new() -> RcVirtualMachine {
        let mut heap = Heap::global();

        let top_level = heap.allocate_empty();
        let integer_proto = heap.allocate_empty();
        let float_proto = heap.allocate_empty();
        let string_proto = heap.allocate_empty();
        let array_proto = heap.allocate_empty();
        let true_proto = heap.allocate_empty();
        let false_proto = heap.allocate_empty();
        let file_proto = heap.allocate_empty();
        let method_proto = heap.allocate_empty();
        let cc_proto = heap.allocate_empty();
        let binding_proto = heap.allocate_empty();

        let true_obj = heap.allocate_empty();
        let false_obj = heap.allocate_empty();

        {
            let true_ref = true_obj.get_mut();
            let false_ref = false_obj.get_mut();

            true_ref.get_mut().set_prototype(true_proto.clone());
            false_ref.get_mut().set_prototype(false_proto.clone());
        }

        let vm = VirtualMachine {
            directories: RwLock::new(Vec::new()),
            executed_files: RwLock::new(HashSet::new()),
            threads: RwLock::new(ThreadList::new()),
            processes: RwLock::new(ProcessList::new()),
            exit_status: RwLock::new(Ok(())),
            global_heap: RwLock::new(heap),
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
            false_object: false_obj
        };

        Arc::new(vm)
    }

    fn allocate_thread(&self, handle: Option<ThreadJoinHandle>) -> RcThread {
        write_lock!(self.threads).add(handle)
    }

    fn allocate_isolated_thread(&self, handle: Option<ThreadJoinHandle>) -> RcThread {
        write_lock!(self.threads).add_isolated(handle)
    }

    fn allocate_process(&self, code: RcCompiledCode, self_obj: ObjectPointer) -> (usize, RcProcess) {
        let mut processes = write_lock!(self.processes);
        let pid = processes.reserve_pid();
        let process = Process::from_code(pid, code, self_obj);

        processes.add(pid, process.clone());

        (pid, process)
    }

    fn allocate_method(&self, process: &RcProcess, receiver: &ObjectPointer,
                       code: RcCompiledCode) -> ObjectPointer {
        let value = object_value::compiled_code(code);
        let proto = self.method_prototype.clone();

        if receiver.is_global() {
            write_lock!(self.global_heap)
                .allocate_value_with_prototype(value, proto)
        }
        else {
            write_lock!(process).allocate(value, proto)
        }
    }
}

impl VirtualMachineMethods for RcVirtualMachine {
    fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        for _ in 0..num_cpus::get() {
            self.start_thread(false);
        }

        let thread = self.allocate_isolated_thread(None);
        let (_, process) = self.allocate_process(code, self.top_level.clone());

        thread.schedule(process);

        self.run_thread(thread);

        *read_lock!(self.exit_status)
    }

    fn run(&self, process: RcProcess, code: RcCompiledCode) -> OptionObjectResult {
        if read_lock!(process).should_pause() {
            return Ok(None);
        }

        let mut skip_until: Option<usize> = None;
        let mut retval = None;

        let mut index = 0;
        let count = code.instructions.len();

        while index < count {
            let ref instruction = code.instructions[index];

            if skip_until.is_some() {
                if index < skip_until.unwrap() {
                    index += 1;

                    continue;
                }
                else {
                    skip_until = None;
                }
            }

            index += 1;

            match instruction.instruction_type {
                InstructionType::SetInteger => {
                    run!(self, ins_set_integer, process, code, instruction);
                },
                InstructionType::SetFloat => {
                    run!(self, ins_set_float, process, code, instruction);
                },
                InstructionType::SetString => {
                    run!(self, ins_set_string, process, code, instruction);
                },
                InstructionType::SetObject => {
                    run!(self, ins_set_object, process, code, instruction);
                },
                InstructionType::SetPrototype => {
                    run!(self, ins_set_prototype, process, code, instruction);
                },
                InstructionType::GetPrototype => {
                    run!(self, ins_get_prototype, process, code, instruction);
                },
                InstructionType::SetArray => {
                    run!(self, ins_set_array, process, code, instruction);
                },
                InstructionType::GetIntegerPrototype => {
                    run!(self, ins_get_integer_prototype, process, code,
                         instruction);
                },
                InstructionType::GetFloatPrototype => {
                    run!(self, ins_get_float_prototype, process, code,
                         instruction);
                },
                InstructionType::GetStringPrototype => {
                    run!(self, ins_get_string_prototype, process, code,
                         instruction);
                },
                InstructionType::GetArrayPrototype => {
                    run!(self, ins_get_array_prototype, process, code,
                         instruction);
                },
                InstructionType::GetTruePrototype => {
                    run!(self, ins_get_true_prototype, process, code,
                         instruction);
                },
                InstructionType::GetFalsePrototype => {
                    run!(self, ins_get_false_prototype, process, code,
                         instruction);
                },
                InstructionType::GetMethodPrototype => {
                    run!(self, ins_get_method_prototype, process, code,
                         instruction);
                },
                InstructionType::GetCompiledCodePrototype => {
                    run!(self, ins_get_compiled_code_prototype, process, code,
                         instruction);
                },
                InstructionType::GetBindingPrototype => {
                    run!(self, ins_get_binding_prototype, process, code,
                         instruction);
                },
                InstructionType::GetTrue => {
                    run!(self, ins_get_true, process, code, instruction);
                },
                InstructionType::GetFalse => {
                    run!(self, ins_get_false, process, code, instruction);
                },
                InstructionType::GetBinding => {
                    run!(self, ins_get_binding, process, code, instruction);
                },
                InstructionType::SetLocal => {
                    run!(self, ins_set_local, process, code, instruction);
                },
                InstructionType::GetLocal => {
                    run!(self, ins_get_local, process, code, instruction);
                },
                InstructionType::LocalExists => {
                    run!(self, ins_local_exists, process, code, instruction);
                },
                InstructionType::SetLiteralConst => {
                    run!(self, ins_set_literal_const, process, code, instruction);
                },
                InstructionType::SetConst => {
                    run!(self, ins_set_const, process, code, instruction);
                },
                InstructionType::GetLiteralConst => {
                    run!(self, ins_get_literal_const, process, code, instruction);
                },
                InstructionType::GetConst => {
                    run!(self, ins_get_const, process, code, instruction);
                },
                InstructionType::LiteralConstExists => {
                    run!(self, ins_literal_const_exists, process, code, instruction);
                },
                InstructionType::SetLiteralAttr => {
                    run!(self, ins_set_literal_attr, process, code, instruction);
                },
                InstructionType::SetAttr => {
                    run!(self, ins_set_attr, process, code, instruction);
                },
                InstructionType::GetLiteralAttr => {
                    run!(self, ins_get_literal_attr, process, code, instruction);
                },
                InstructionType::GetAttr => {
                    run!(self, ins_get_attr, process, code, instruction);
                },
                InstructionType::LiteralAttrExists => {
                    run!(self, ins_literal_attr_exists, process, code, instruction);
                },
                InstructionType::SetCompiledCode => {
                    run!(self, ins_set_compiled_code, process, code,
                         instruction);
                },
                InstructionType::SendLiteral => {
                    run!(self, ins_send_literal, process, code, instruction);
                },
                InstructionType::Send => {
                    run!(self, ins_send, process, code, instruction);
                },
                InstructionType::LiteralRespondsTo => {
                    run!(self, ins_literal_responds_to, process, code, instruction);
                },
                InstructionType::RespondsTo => {
                    run!(self, ins_responds_to, process, code, instruction);
                },
                InstructionType::Return => {
                    retval = run!(self, ins_return, process, code, instruction);
                },
                InstructionType::GotoIfFalse => {
                    skip_until = run!(self, ins_goto_if_false, process, code,
                                      instruction);
                },
                InstructionType::GotoIfTrue => {
                    skip_until = run!(self, ins_goto_if_true, process, code,
                                      instruction);
                },
                InstructionType::Goto => {
                    index = run!(self, ins_goto, process, code, instruction);
                },
                InstructionType::DefMethod => {
                    run!(self, ins_def_method, process, code, instruction);
                },
                InstructionType::DefLiteralMethod => {
                    run!(self, ins_def_literal_method, process, code,
                         instruction);
                },
                InstructionType::RunCode => {
                    run!(self, ins_run_code, process, code, instruction);
                },
                InstructionType::RunLiteralCode => {
                    run!(self, ins_run_literal_code, process, code, instruction);
                },
                InstructionType::GetToplevel => {
                    run!(self, ins_get_toplevel, process, code, instruction);
                },
                InstructionType::GetSelf => {
                    run!(self, ins_get_self, process, code, instruction);
                },
                InstructionType::IsError => {
                    run!(self, ins_is_error, process, code, instruction);
                },
                InstructionType::ErrorToString => {
                    run!(self, ins_error_to_integer, process, code, instruction);
                },
                InstructionType::IntegerAdd => {
                    run!(self, ins_integer_add, process, code, instruction);
                },
                InstructionType::IntegerDiv => {
                    run!(self, ins_integer_div, process, code, instruction);
                },
                InstructionType::IntegerMul => {
                    run!(self, ins_integer_mul, process, code, instruction);
                },
                InstructionType::IntegerSub => {
                    run!(self, ins_integer_sub, process, code, instruction);
                },
                InstructionType::IntegerMod => {
                    run!(self, ins_integer_mod, process, code, instruction);
                },
                InstructionType::IntegerToFloat => {
                    run!(self, ins_integer_to_float, process, code, instruction);
                },
                InstructionType::IntegerToString => {
                    run!(self, ins_integer_to_string, process, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseAnd => {
                    run!(self, ins_integer_bitwise_and, process, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseOr => {
                    run!(self, ins_integer_bitwise_or, process, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseXor => {
                    run!(self, ins_integer_bitwise_xor, process, code,
                         instruction);
                },
                InstructionType::IntegerShiftLeft => {
                    run!(self, ins_integer_shift_left, process, code,
                         instruction);
                },
                InstructionType::IntegerShiftRight => {
                    run!(self, ins_integer_shift_right, process, code,
                         instruction);
                },
                InstructionType::IntegerSmaller => {
                    run!(self, ins_integer_smaller, process, code, instruction);
                },
                InstructionType::IntegerGreater => {
                    run!(self, ins_integer_greater, process, code, instruction);
                },
                InstructionType::IntegerEquals => {
                    run!(self, ins_integer_equals, process, code, instruction);
                },
                InstructionType::SpawnLiteralProcess => {
                    run!(self, ins_spawn_literal_process, process, code,
                         instruction);
                },
                InstructionType::FloatAdd => {
                    run!(self, ins_float_add, process, code, instruction);
                },
                InstructionType::FloatMul => {
                    run!(self, ins_float_mul, process, code, instruction);
                },
                InstructionType::FloatDiv => {
                    run!(self, ins_float_div, process, code, instruction);
                },
                InstructionType::FloatSub => {
                    run!(self, ins_float_sub, process, code, instruction);
                },
                InstructionType::FloatMod => {
                    run!(self, ins_float_mod, process, code, instruction);
                },
                InstructionType::FloatToInteger => {
                    run!(self, ins_float_to_integer, process, code, instruction);
                },
                InstructionType::FloatToString => {
                    run!(self, ins_float_to_string, process, code, instruction);
                },
                InstructionType::FloatSmaller => {
                    run!(self, ins_float_smaller, process, code, instruction);
                },
                InstructionType::FloatGreater => {
                    run!(self, ins_float_greater, process, code, instruction);
                },
                InstructionType::FloatEquals => {
                    run!(self, ins_float_equals, process, code, instruction);
                },
                InstructionType::ArrayInsert => {
                    run!(self, ins_array_insert, process, code, instruction);
                },
                InstructionType::ArrayAt => {
                    run!(self, ins_array_at, process, code, instruction);
                },
                InstructionType::ArrayRemove => {
                    run!(self, ins_array_remove, process, code, instruction);
                },
                InstructionType::ArrayLength => {
                    run!(self, ins_array_length, process, code, instruction);
                },
                InstructionType::ArrayClear => {
                    run!(self, ins_array_clear, process, code, instruction);
                },
                InstructionType::StringToLower => {
                    run!(self, ins_string_to_lower, process, code, instruction);
                },
                InstructionType::StringToUpper => {
                    run!(self, ins_string_to_upper, process, code, instruction);
                },
                InstructionType::StringEquals => {
                    run!(self, ins_string_equals, process, code, instruction);
                },
                InstructionType::StringToBytes => {
                    run!(self, ins_string_to_bytes, process, code, instruction);
                },
                InstructionType::StringFromBytes => {
                    run!(self, ins_string_from_bytes, process, code, instruction);
                },
                InstructionType::StringLength => {
                    run!(self, ins_string_length, process, code, instruction);
                },
                InstructionType::StringSize => {
                    run!(self, ins_string_size, process, code, instruction);
                },
                InstructionType::StdoutWrite => {
                    run!(self, ins_stdout_write, process, code, instruction);
                },
                InstructionType::StderrWrite => {
                    run!(self, ins_stderr_write, process, code, instruction);
                },
                InstructionType::StdinRead => {
                    run!(self, ins_stdin_read, process, code, instruction);
                },
                InstructionType::StdinReadLine => {
                    run!(self, ins_stdin_read_line, process, code, instruction);
                },
                InstructionType::FileOpen => {
                    run!(self, ins_file_open, process, code, instruction);
                },
                InstructionType::FileWrite => {
                    run!(self, ins_file_write, process, code, instruction);
                },
                InstructionType::FileRead => {
                    run!(self, ins_file_read, process, code, instruction);
                },
                InstructionType::FileReadLine => {
                    run!(self, ins_file_read_line, process, code, instruction);
                },
                InstructionType::FileFlush => {
                    run!(self, ins_file_flush, process, code, instruction);
                },
                InstructionType::FileSize => {
                    run!(self, ins_file_size, process, code, instruction);
                },
                InstructionType::FileSeek => {
                    run!(self, ins_file_seek, process, code, instruction);
                },
                InstructionType::RunLiteralFile => {
                    run!(self, ins_run_literal_file, process, code, instruction);
                },
                InstructionType::RunFile => {
                    run!(self, ins_run_file, process, code, instruction);
                },
                InstructionType::GetCaller => {
                    run!(self, ins_get_caller, process, code, instruction);
                },
                InstructionType::SetOuterScope => {
                    run!(self, ins_set_outer_scope, process, code, instruction);
                },
                InstructionType::SpawnProcess => {
                    run!(self, ins_spawn_process, process, code, instruction);
                },
                InstructionType::SendProcessMessage => {
                    run!(self, ins_send_process_message, process, code,
                         instruction);
                },
                InstructionType::ReceiveProcessMessage => {
                    run!(self, ins_receive_process_message, process, code,
                         instruction);
                },
                InstructionType::GetCurrentProcessPid => {
                    run!(self, ins_get_current_process_pid, process, code,
                         instruction);
                },
            };
        }

        Ok(retval)
    }

    fn ins_set_integer(&self, process: RcProcess, code: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index    = try!(instruction.arg(1));
        let value    = *try!(code.integer(index));

        let obj = write_lock!(process)
            .allocate(object_value::integer(value), self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_float(&self, process: RcProcess, code: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let value = *try!(code.float(index));

        let obj = write_lock!(process)
            .allocate(object_value::float(value), self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_string(&self, process: RcProcess, code: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let value = try!(code.string(index));

        let obj = write_lock!(process)
            .allocate(object_value::string(value.clone()),
                      self.string_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_object(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let is_global = try!(instruction.arg(1)) == 1;

        let obj = if is_global {
            write_lock!(self.global_heap).allocate_empty()
        }
        else {
            write_lock!(process).allocate_empty()
        };

        if let Ok(proto_index) = instruction.arg(2) {
            let mut proto = try!(read_lock!(process).get_register(proto_index));

            if is_global && proto.is_local() {
                proto = write_lock!(self.global_heap).copy_object(proto);
            }

            let obj_ref = obj.get_mut();

            obj_ref.get_mut().set_prototype(proto);
        }

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_prototype(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let source = instruction_object!(instruction, process, 0);
        let proto = instruction_object!(instruction, process, 1);

        let source_ref = source.get_mut();

        source_ref.get_mut().set_prototype(proto);

        Ok(())
    }

    fn ins_get_prototype(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);

        let source_ref = source.get();
        let source_obj = source_ref.get();

        let proto = try!(source_obj.prototype().ok_or_else(|| format!(
            "The object in register {} does not have a prototype",
            instruction.arguments[1]
        )));

        write_lock!(process).set_register(register, proto);

        Ok(())
    }

    fn ins_set_array(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let val_count = instruction.arguments.len() - 1;

        let values = try!(
            self.collect_arguments(process.clone(), instruction, 1, val_count)
        );

        let obj = write_lock!(process)
            .allocate(object_value::array(values), self.array_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_get_integer_prototype(&self, process: RcProcess, _: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.integer_prototype.clone());

        Ok(())
    }

    fn ins_get_float_prototype(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.float_prototype.clone());

        Ok(())
    }

    fn ins_get_string_prototype(&self, process: RcProcess, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.string_prototype.clone());

        Ok(())
    }

    fn ins_get_array_prototype(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.array_prototype.clone());

        Ok(())
    }

    fn ins_get_true_prototype(&self, process: RcProcess, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.true_prototype.clone());

        Ok(())
    }

    fn ins_get_false_prototype(&self, process: RcProcess, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.false_prototype.clone());

        Ok(())
    }

    fn ins_get_method_prototype(&self, process: RcProcess, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.method_prototype.clone());

        Ok(())
    }

    fn ins_get_binding_prototype(&self, process: RcProcess, _: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.binding_prototype.clone());

        Ok(())
    }

    fn ins_get_compiled_code_prototype(&self, process: RcProcess, _: RcCompiledCode,
                                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process)
            .set_register(register, self.compiled_code_prototype.clone());

        Ok(())
    }

    fn ins_get_true(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.true_object.clone());

        Ok(())
    }

    fn ins_get_false(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.false_object.clone());

        Ok(())
    }

    fn ins_get_binding(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let ref frame = read_lock!(process).call_frame;

        let obj = write_lock!(process)
            .allocate(object_value::binding(frame.binding.clone()),
                      self.binding_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_local(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let local_index = try!(instruction.arg(0));
        let object = instruction_object!(instruction, process, 1);

        write_lock!(process).set_local(local_index, object);

        Ok(())
    }

    fn ins_get_local(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let local_index = try!(instruction.arg(1));

        let object = try!(read_lock!(process).get_local(local_index));

        write_lock!(process).set_register(register, object);

        Ok(())
    }

    fn ins_local_exists(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let local_index = try!(instruction.arg(1));

        let value = if read_lock!(process).local_exists(local_index) {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, value);

        Ok(())
    }

    fn ins_set_literal_const(&self, process: RcProcess, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_index = try!(instruction.arg(1));
        let source_ptr = instruction_object!(instruction, process, 2);
        let name = try!(code.string(name_index));

        let source = copy_if_global!(self.global_heap, source_ptr, target_ptr);
        let target_ref = target_ptr.get_mut();

        target_ref.get_mut().add_constant(name.clone(), source);

        Ok(())
    }

    fn ins_set_const(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name = instruction_object!(instruction, process, 1);
        let source_ptr = instruction_object!(instruction, process, 2);

        let name_ref = name.get();
        let name_obj = name_ref.get();

        ensure_strings!(name_obj);

        let name_str = name_obj.value.as_string().clone();
        let source = copy_if_global!(self.global_heap, source_ptr, target_ptr);

        let target_ref = target_ptr.get_mut();
        let target = target_ref.get_mut();

        target.add_constant(name_str, source);

        Ok(())
    }

    fn ins_get_literal_const(&self, process: RcProcess, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let src = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let src_ref = src.get();

        let object = try!(
            src_ref.get().lookup_constant(name)
                .ok_or_else(|| constant_error!(instruction.arguments[1], name))
        );

        write_lock!(process).set_register(register, object);

        Ok(())
    }

    fn ins_get_const(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let src = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_ref = name.get();
        let name_obj = name_ref.get();
        let src_ref = src.get();

        ensure_strings!(name_obj);

        let name_str = name_obj.value.as_string();

        let object = try!(
            src_ref.get().lookup_constant(name_str)
                .ok_or_else(|| constant_error!(instruction.arguments[1], name_str))
        );

        write_lock!(process).set_register(register, object);

        Ok(())
    }

    fn ins_literal_const_exists(&self, process: RcProcess, code: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let source_ref = source.get();
        let constant = source_ref.get().lookup_constant(name);

        if constant.is_some() {
            write_lock!(process).set_register(register, self.true_object.clone());
        }
        else {
            write_lock!(process).set_register(register, self.false_object.clone());
        }

        Ok(())
    }

    fn ins_set_literal_attr(&self, process: RcProcess, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_index = try!(instruction.arg(1));
        let value_ptr = instruction_object!(instruction, process, 2);

        let name = try!(code.string(name_index));
        let value = copy_if_global!(self.global_heap, value_ptr, target_ptr);

        let target_ref = target_ptr.get_mut();

        target_ref.get_mut().add_attribute(name.clone(), value);

        Ok(())
    }

    fn ins_set_attr(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let name_ptr = instruction_object!(instruction, process, 1);
        let value_ptr = instruction_object!(instruction, process, 2);

        let name_ref = name_ptr.get();
        let name_obj = name_ref.get();

        ensure_strings!(name_obj);

        let name = name_obj.value.as_string();
        let value = copy_if_global!(self.global_heap, value_ptr, target_ptr);

        let target_ref = target_ptr.get_mut();

        target_ref.get_mut().add_attribute(name.clone(), value);

        Ok(())
    }

    fn ins_get_literal_attr(&self, process: RcProcess, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));

        let source_ref = source.get();
        let name = try!(code.string(name_index));

        let attr = try!(
            source_ref.get().lookup_attribute(name)
                .ok_or_else(|| attribute_error!(instruction.arguments[1], name))
        );

        write_lock!(process).set_register(register, attr);

        Ok(())
    }

    fn ins_get_attr(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_ref = name.get();
        let name_obj = name_ref.get();
        let source_ref = source.get();

        ensure_strings!(name_obj);

        let name = name_obj.value.as_string();

        let attr = try!(
            source_ref.get().lookup_attribute(name)
                .ok_or_else(|| attribute_error!(instruction.arguments[1], name))
        );

        write_lock!(process).set_register(register, attr);

        Ok(())
    }

    fn ins_literal_attr_exists(&self, process: RcProcess, code: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source_ptr = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let source_ref = source_ptr.get();
        let source = source_ref.get();

        let obj = if source.has_attribute(name) {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_set_compiled_code(&self, process: RcProcess, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let cc_index = try!(instruction.arg(1));

        let cc = try!(code.code_object(cc_index));

        let obj = write_lock!(process)
            .allocate(object_value::compiled_code(cc),
                      self.compiled_code_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_send_literal(&self, process: RcProcess, code: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        self.send_message(name, process, instruction)
    }

    fn ins_send(&self, process: RcProcess, _: RcCompiledCode,
                instruction: &Instruction) -> EmptyResult {
        let string = instruction_object!(instruction, process, 2);
        let string_ref = string.get();
        let string_obj = string_ref.get();

        ensure_strings!(string_obj);

        self.send_message(string_obj.value.as_string(), process, instruction)
    }

    fn ins_literal_responds_to(&self, process: RcProcess, code: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let source_ref = source.get();
        let source_obj = source_ref.get();

        let result = if source_obj.responds_to(name) {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_responds_to(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, process, 1);
        let name = instruction_object!(instruction, process, 2);

        let name_ref = name.get();
        let name_obj = name_ref.get();

        let source_ref = source.get();
        let source_obj = source_ref.get();

        ensure_strings!(name_obj);

        let result = if source_obj.responds_to(name_obj.value.as_string()) {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_return(&self, process: RcProcess, _: RcCompiledCode,
                  instruction: &Instruction) -> OptionObjectResult {
        let register = try!(instruction.arg(0));

        Ok(read_lock!(process).get_register_option(register))
    }

    fn ins_goto_if_false(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> OptionIntegerResult {
        let go_to = try!(instruction.arg(0));
        let value_reg = try!(instruction.arg(1));
        let value = read_lock!(process).get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if obj == self.false_object.clone() {
                    Some(go_to)
                }
                else {
                    None
                }
            },
            None => { Some(go_to) }
        };

        Ok(matched)
    }

    fn ins_goto_if_true(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> OptionIntegerResult {
        let go_to = try!(instruction.arg(0));
        let value_reg = try!(instruction.arg(1));
        let value = read_lock!(process).get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if obj == self.false_object.clone() {
                    None
                }
                else {
                    Some(go_to)
                }
            },
            None => { None }
        };

        Ok(matched)
    }

    fn ins_goto(&self, _: RcProcess, _: RcCompiledCode,
                instruction: &Instruction) -> IntegerResult {
        let go_to = try!(instruction.arg(0));

        Ok(go_to)
    }

    fn ins_def_method(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let name_ptr = instruction_object!(instruction, process, 2);
        let cc_ptr = instruction_object!(instruction, process, 3);

        let receiver_ref = receiver_ptr.get_mut();
        let mut receiver = receiver_ref.get_mut();

        let name_ref = name_ptr.get();
        let name_obj = name_ref.get();

        ensure_strings!(name_obj);

        let cc_ref = cc_ptr.get();
        let cc_obj = cc_ref.get();

        ensure_compiled_code!(cc_obj);

        let name = name_obj.value.as_string();
        let cc = cc_obj.value.as_compiled_code();

        let method = self.allocate_method(&process, &receiver_ptr, cc);

        receiver.add_method(name.clone(), method.clone());

        write_lock!(process).set_register(register, method);

        Ok(())
    }

    fn ins_def_literal_method(&self, process: RcProcess, code: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let name_index = try!(instruction.arg(2));
        let cc_index = try!(instruction.arg(3));

        let name = try!(code.string(name_index));
        let cc = try!(code.code_object(cc_index));

        let receiver_ref = receiver_ptr.get_mut();
        let mut receiver = receiver_ref.get_mut();

        let method = self.allocate_method(&process, &receiver_ptr, cc);

        receiver.add_method(name.clone(), method.clone());

        write_lock!(process).set_register(register, method);

        Ok(())
    }

    fn ins_run_code(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let cc_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let code_obj = {
            let cc_ref = cc_ptr.get();
            let cc_obj = cc_ref.get();

            ensure_compiled_code!(cc_obj);

            cc_obj.value.as_compiled_code()
        };

        let arg_ref = arg_ptr.get();
        let arg_obj = arg_ref.get();

        ensure_integers!(arg_obj);

        let arg_count = arg_obj.value.as_integer() as usize;

        let arguments = try!(
            self.collect_arguments(process.clone(), instruction, 3, arg_count)
        );

        let binding_idx = 3 + arg_count;

        let binding = if let Ok(binding_reg) = instruction.arg(binding_idx) {
            let obj_ptr = instruction_object!(instruction, process, binding_reg);

            let obj_ref = obj_ptr.get();
            let obj = obj_ref.get();

            if !obj.value.is_binding() {
                return Err(format!("Argument {} is not a valid Binding",
                                   binding_idx));
            }

            Some(obj.value.as_binding())
        }
        else {
            None
        };

        let retval = try!(
            self.run_code(process.clone(), code_obj, cc_ptr, arguments, binding)
        );

        if retval.is_some() {
            write_lock!(process).set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn ins_run_literal_code(&self, process: RcProcess, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let code_index = try!(instruction.arg(1));
        let receiver = instruction_object!(instruction, process, 2);
        let code_obj = try!(code.code_object(code_index));

        let retval = try!(
            self.run_code(process.clone(), code_obj, receiver, Vec::new(), None)
        );

        if retval.is_some() {
            write_lock!(process).set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn ins_get_toplevel(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        write_lock!(process).set_register(register, self.top_level.clone());

        Ok(())
    }

    fn ins_get_self(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        let self_object = read_lock!(process).call_frame.self_object();

        write_lock!(process).set_register(register, self_object);

        Ok(())
    }

    fn ins_is_error(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let obj_ptr = instruction_object!(instruction, process, 1);

        let obj_ref = obj_ptr.get();
        let obj = obj_ref.get();

        let result = if obj.value.is_error() {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_error_to_integer(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let error_ptr = instruction_object!(instruction, process, 1);

        let error_ref = error_ptr.get();
        let error = error_ref.get();

        let proto = self.integer_prototype.clone();
        let integer = error.value.as_error() as i64;

        let result = write_lock!(process)
            .allocate(object_value::integer(integer), proto);

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_integer_add(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() + arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_div(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() / arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_mul(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() * arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_sub(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() - arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_mod(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() % arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_to_float(&self, process: RcProcess, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let integer_ptr = instruction_object!(instruction, process, 1);

        let integer_ref = integer_ptr.get();
        let integer = integer_ref.get();

        ensure_integers!(integer);

        let result = integer.value.as_integer() as f64;

        let obj = write_lock!(process)
            .allocate(object_value::float(result),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_to_string(&self, process: RcProcess, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let integer_ptr = instruction_object!(instruction, process, 1);

        let integer_ref = integer_ptr.get();
        let integer = integer_ref.get();

        ensure_integers!(integer);

        let result = integer.value.as_integer().to_string();

        let obj = write_lock!(process)
            .allocate(object_value::string(result),
                      self.string_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_and(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() & arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_or(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() | arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_xor(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() ^ arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_shift_left(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() << arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_shift_right(&self, process: RcProcess, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() >> arg.value.as_integer();

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_integer_smaller(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() < arg.value.as_integer();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_integer_greater(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() > arg.value.as_integer();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_integer_equals(&self, process: RcProcess, _: RcCompiledCode,
                          instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() == arg.value.as_integer();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_spawn_literal_process(&self, process: RcProcess, code: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let code_index = try!(instruction.arg(1));

        let isolated = if let Ok(num) = instruction.arg(2) {
            num == 1
        }
        else {
            false
        };

        let code_obj = try!(code.code_object(code_index));

        self.spawn_process(process, code_obj, register, isolated);

        Ok(())
    }

    fn ins_spawn_process(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let code_ptr = instruction_object!(instruction, process, 1);

        let isolated = if instruction.arg(2).is_ok() {
            let isolated_ptr = instruction_object!(instruction, process, 2);

            isolated_ptr != self.false_object.clone()
        }
        else {
            false
        };

        let code_ref = code_ptr.get();
        let code = code_ref.get();

        ensure_compiled_code!(code);

        let code_obj = code.value.as_compiled_code();

        self.spawn_process(process, code_obj, register, isolated);

        Ok(())
    }

    fn ins_send_process_message(&self, process: RcProcess, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let pid_ptr = instruction_object!(instruction, process, 1);
        let msg_ptr = instruction_object!(instruction, process, 2);

        let pid = {
            let pid_ref = pid_ptr.get();
            let pid_obj = pid_ref.get();

            ensure_integers!(pid_obj);

            pid_obj.value.as_integer() as usize
        };

        if let Some(receiver) = read_lock!(self.processes).get(pid) {
            let inbox = read_lock!(receiver).inbox();
            let mut to_send = msg_ptr.clone();

            // Local objects need to be deep copied.
            if msg_ptr.is_local() {
                to_send = write_lock!(receiver).copy_object(to_send);
            }

            inbox.send(to_send);

            write_lock!(process).set_register(register, msg_ptr);
        }

        Ok(())
    }

    fn ins_receive_process_message(&self, process: RcProcess, _: RcCompiledCode,
                                   instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let pid = read_lock!(process).pid;
        let source = read_lock!(self.processes).get(pid).unwrap();
        let inbox = read_lock!(source).inbox();
        let msg_ptr = inbox.receive();

        write_lock!(process).set_register(register, msg_ptr);

        Ok(())
    }

    fn ins_get_current_process_pid(&self, process: RcProcess, _: RcCompiledCode,
                                   instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let pid = read_lock!(process).pid;

        let mut proc_guard = write_lock!(process);
        let pid_obj = proc_guard.allocate(object_value::integer(pid as i64),
                                          self.integer_prototype.clone());

        proc_guard.set_register(register, pid_obj);

        Ok(())
    }

    fn ins_float_add(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let added = receiver.value.as_float() + arg.value.as_float();

        let obj = write_lock!(process)
            .allocate(object_value::float(added),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_mul(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() * arg.value.as_float();

        let obj = write_lock!(process)
            .allocate(object_value::float(result),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_div(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() / arg.value.as_float();

        let obj = write_lock!(process)
            .allocate(object_value::float(result),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_sub(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() - arg.value.as_float();

        let obj = write_lock!(process)
            .allocate(object_value::float(result),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_mod(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() % arg.value.as_float();

        let obj = write_lock!(process)
            .allocate(object_value::float(result),
                      self.float_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_to_integer(&self, process: RcProcess, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let float_ptr = instruction_object!(instruction, process, 1);

        let float_ref = float_ptr.get();
        let float = float_ref.get();

        ensure_floats!(float);

        let result = float.value.as_float() as i64;

        let obj = write_lock!(process)
            .allocate(object_value::integer(result),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_to_string(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let float_ptr = instruction_object!(instruction, process, 1);

        let float_ref = float_ptr.get();
        let float = float_ref.get();

        ensure_floats!(float);

        let result = float.value.as_float().to_string();

        let obj = write_lock!(process)
            .allocate(object_value::string(result),
                      self.string_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_float_smaller(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() < arg.value.as_float();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_float_greater(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() > arg.value.as_float();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_float_equals(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() == arg.value.as_float();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_array_insert(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);
        let value_ptr = instruction_object!(instruction, process, 3);

        let array_ref = array_ptr.get_mut();
        let mut array = array_ref.get_mut();

        ensure_arrays!(array);

        let index_ref = index_ptr.get();
        let index_obj = index_ref.get();

        ensure_integers!(index_obj);

        let mut vector = array.value.as_array_mut();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(vector, index);

        let value = copy_if_global!(self.global_heap, value_ptr, array_ptr);

        vector.insert(index, value.clone());

        write_lock!(process).set_register(register, value);

        Ok(())
    }

    fn ins_array_at(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);

        let array_ref = array_ptr.get();
        let array = array_ref.get();

        let index_ref = index_ptr.get();
        let index_obj = index_ref.get();

        ensure_arrays!(array);
        ensure_integers!(index_obj);

        let vector = array.value.as_array();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(vector, index);

        let value = vector[index].clone();

        write_lock!(process).set_register(register, value);

        Ok(())
    }

    fn ins_array_remove(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let array_ptr = instruction_object!(instruction, process, 1);
        let index_ptr = instruction_object!(instruction, process, 2);

        let array_ref = array_ptr.get_mut();
        let mut array = array_ref.get_mut();

        let index_ref = index_ptr.get();
        let index_obj = index_ref.get();

        ensure_arrays!(array);
        ensure_integers!(index_obj);

        let mut vector = array.value.as_array_mut();
        let index = int_to_vector_index!(vector, index_obj.value.as_integer());

        ensure_array_within_bounds!(vector, index);

        let value = vector.remove(index);

        write_lock!(process).set_register(register, value);

        Ok(())
    }

    fn ins_array_length(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let array_ptr = instruction_object!(instruction, process, 1);

        let array_ref = array_ptr.get();
        let array = array_ref.get();

        ensure_arrays!(array);

        let vector = array.value.as_array();
        let length = vector.len() as i64;

        let obj = write_lock!(process)
            .allocate(object_value::integer(length),
                      self.integer_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_array_clear(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let array_ptr = instruction_object!(instruction, process, 0);

        let array_ref = array_ptr.get_mut();
        let mut array = array_ref.get_mut();

        ensure_arrays!(array);

        let mut vector = array.value.as_array_mut();

        vector.clear();

        Ok(())
    }

    fn ins_string_to_lower(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source_ptr = instruction_object!(instruction, process, 1);

        let source_ref = source_ptr.get();
        let source = source_ref.get();

        ensure_strings!(source);

        let lower = source.value.as_string().to_lowercase();

        let obj = write_lock!(process)
            .allocate(object_value::string(lower),
                      self.string_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_string_to_upper(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source_ptr = instruction_object!(instruction, process, 1);

        let source_ref = source_ptr.get();
        let source = source_ref.get();

        ensure_strings!(source);

        let upper = source.value.as_string().to_uppercase();

        let obj = write_lock!(process)
            .allocate(object_value::string(upper),
                      self.string_prototype.clone());

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_string_equals(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let arg_ptr = instruction_object!(instruction, process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(receiver, arg);

        let result = receiver.value.as_string() == arg.value.as_string();

        let boolean = if result {
            self.true_object.clone()
        }
        else {
            self.false_object.clone()
        };

        write_lock!(process).set_register(register, boolean);

        Ok(())
    }

    fn ins_string_to_bytes(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(arg);

        let int_proto   = self.integer_prototype.clone();
        let array_proto = self.array_prototype.clone();

        let array = arg.value.as_string().as_bytes().iter().map(|&b| {
            write_lock!(process)
                .allocate(object_value::integer(b as i64), int_proto.clone())
        }).collect::<Vec<_>>();

        let obj = write_lock!(process)
            .allocate(object_value::array(array), array_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_string_from_bytes(&self, process: RcProcess, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_arrays!(arg);

        let string_proto = self.string_prototype.clone();
        let array        = arg.value.as_array();

        for int_ptr in array.iter() {
            let int_ref = int_ptr.get();
            let int = int_ref.get();

            ensure_integers!(int);
        }

        let bytes = arg.value.as_array().iter().map(|ref int_ptr| {
            let int_ref = int_ptr.get();

            int_ref.get().value.as_integer() as u8
        }).collect::<Vec<_>>();

        let string = try_error!(try_from_utf8!(bytes), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::string(string), string_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_string_length(&self, process: RcProcess, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(arg);

        let int_proto = self.integer_prototype.clone();
        let length = arg.value.as_string().chars().count() as i64;

        let obj = write_lock!(process)
            .allocate(object_value::integer(length), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_string_size(&self, process: RcProcess, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(arg);

        let int_proto = self.integer_prototype.clone();
        let size = arg.value.as_string().len() as i64;

        let obj = write_lock!(process)
            .allocate(object_value::integer(size), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_stdout_write(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(arg);

        let int_proto = self.integer_prototype.clone();
        let mut stdout = io::stdout();

        let result = try_io!(stdout.write(arg.value.as_string().as_bytes()),
                             process, register);

        try_io!(stdout.flush(), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::integer(result as i64), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_stderr_write(&self, process: RcProcess, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_ptr = instruction_object!(instruction, process, 1);

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        ensure_strings!(arg);

        let int_proto = self.integer_prototype.clone();
        let mut stderr = io::stderr();

        let result = try_io!(stderr.write(arg.value.as_string().as_bytes()),
                             process, register);

        try_io!(stderr.flush(), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::integer(result as i64), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_stdin_read(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let proto = self.string_prototype.clone();

        let mut buffer = file_reading_buffer!(instruction, process, 1);

        try_io!(io::stdin().read_to_string(&mut buffer), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::string(buffer), proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_stdin_read_line(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let proto = self.string_prototype.clone();

        let mut buffer = String::new();

        try_io!(io::stdin().read_line(&mut buffer), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::string(buffer), proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_file_open(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let path_ptr = instruction_object!(instruction, process, 1);
        let mode_ptr = instruction_object!(instruction, process, 2);

        let file_proto = self.file_prototype.clone();

        let path_ref = path_ptr.get();
        let path = path_ref.get();

        let mode_ref = mode_ptr.get();
        let mode = mode_ref.get();

        let path_string = path.value.as_string();
        let mode_string = mode.value.as_string().as_ref();
        let mut open_opts = OpenOptions::new();

        match mode_string {
            "r"  => open_opts.read(true),
            "r+" => open_opts.read(true).write(true).truncate(true).create(true),
            "w"  => open_opts.write(true).truncate(true).create(true),
            "w+" => open_opts.read(true).write(true).truncate(true).create(true),
            "a"  => open_opts.append(true).create(true),
            "a+" => open_opts.read(true).append(true).create(true),
            _    => set_error!(errors::IO_INVALID_OPEN_MODE, process, register)
        };

        let file = try_io!(open_opts.open(path_string), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::file(file), file_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_file_write(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);
        let string_ptr = instruction_object!(instruction, process, 2);

        let file_ref = file_ptr.get_mut();
        let mut file = file_ref.get_mut();

        let string_ref = string_ptr.get();
        let string = string_ref.get();

        ensure_files!(file);
        ensure_strings!(string);

        let int_proto = self.integer_prototype.clone();
        let mut file = file.value.as_file_mut();
        let bytes = string.value.as_string().as_bytes();

        let result = try_io!(file.write(bytes), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::integer(result as i64), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_file_read(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);

        let file_ref = file_ptr.get_mut();
        let mut file_obj = file_ref.get_mut();

        ensure_files!(file_obj);

        let mut buffer = file_reading_buffer!(instruction, process, 2);
        let int_proto = self.integer_prototype.clone();
        let mut file = file_obj.value.as_file_mut();

        try_io!(file.read_to_string(&mut buffer), process, register);

        let obj = write_lock!(process)
            .allocate(object_value::string(buffer), int_proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_file_read_line(&self, process: RcProcess, _: RcCompiledCode,
                          instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);

        let file_ref = file_ptr.get_mut();
        let mut file_obj = file_ref.get_mut();

        ensure_files!(file_obj);

        let proto = self.string_prototype.clone();
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

        let obj = write_lock!(process)
            .allocate(object_value::string(string), proto);

        write_lock!(process).set_register(register, obj);

        Ok(())
    }

    fn ins_file_flush(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);

        let file_ref = file_ptr.get_mut();
        let mut file_obj = file_ref.get_mut();

        ensure_files!(file_obj);

        let mut file = file_obj.value.as_file_mut();

        try_io!(file.flush(), process, register);

        write_lock!(process).set_register(register, self.true_object.clone());

        Ok(())
    }

    fn ins_file_size(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);

        let file_ref = file_ptr.get();
        let file_obj = file_ref.get();

        ensure_files!(file_obj);

        let file = file_obj.value.as_file();
        let meta = try_io!(file.metadata(), process, register);

        let size = meta.len() as i64;
        let proto = self.integer_prototype.clone();

        let result = write_lock!(process)
            .allocate(object_value::integer(size), proto);

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_file_seek(&self, process: RcProcess, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let file_ptr = instruction_object!(instruction, process, 1);
        let offset_ptr = instruction_object!(instruction, process, 2);

        let file_ref = file_ptr.get_mut();
        let mut file_obj = file_ref.get_mut();

        let offset_ref = offset_ptr.get();
        let offset_obj = offset_ref.get();

        ensure_files!(file_obj);
        ensure_integers!(offset_obj);

        let mut file = file_obj.value.as_file_mut();
        let offset = offset_obj.value.as_integer();

        ensure_positive_read_size!(offset);

        let seek_from = SeekFrom::Start(offset as u64);
        let new_offset = try_io!(file.seek(seek_from), process, register);

        let proto = self.integer_prototype.clone();

        let result = write_lock!(process)
            .allocate(object_value::integer(new_offset as i64), proto);

        write_lock!(process).set_register(register, result);

        Ok(())
    }

    fn ins_run_literal_file(&self, process: RcProcess, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let path = try!(code.string(index));

        self.run_file(path, process, register)
    }

    fn ins_run_file(&self, process: RcProcess, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let path_ptr = instruction_object!(instruction, process, 1);

        let path_ref = path_ptr.get();
        let path = path_ref.get();

        ensure_strings!(path);

        self.run_file(path.value.as_string(), process, register)
    }

    fn ins_get_caller(&self, process: RcProcess, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        let caller = {
            let ref frame = read_lock!(process).call_frame;

            if let Some(parent) = frame.parent() {
                parent.self_object()
            }
            else {
                frame.self_object()
            }
        };

        write_lock!(process).set_register(register, caller);

        Ok(())
    }

    fn ins_set_outer_scope(&self, process: RcProcess, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let target_ptr = instruction_object!(instruction, process, 0);
        let scope_ptr = instruction_object!(instruction, process, 1);

        let target_ref = target_ptr.get_mut();
        let mut target = target_ref.get_mut();

        let scope = copy_if_global!(self.global_heap, scope_ptr, target_ptr);

        target.set_outer_scope(scope);

        Ok(())
    }

    fn error(&self, process: RcProcess, message: String) {
        let mut stderr = io::stderr();
        let mut error = message.to_string();
        let ref frame = read_lock!(process).call_frame;

        *write_lock!(self.exit_status) = Err(());

        frame.each_frame(|frame| {
            error.push_str(&format!(
                "\n{} line {} in \"{}\"",
                frame.file,
                frame.line,
                frame.name
            ));
        });

        write!(&mut stderr, "Fatal error:\n\n{}\n\n", error).unwrap();

        stderr.flush().unwrap();
    }

    fn run_code(&self,
                process: RcProcess,
                code: RcCompiledCode,
                self_obj: ObjectPointer,
                args: Vec<ObjectPointer>,
                binding: Option<RcBinding>) -> OptionObjectResult {
        // Scoped so the the RwLock is local to the block, allowing recursive
        // calling of the "run" method.
        {
            let frame = if let Some(rc_bind) = binding {
                CallFrame::from_code_with_binding(code.clone(), rc_bind)
            }
            else {
                CallFrame::from_code(code.clone(), self_obj)
            };

            let mut plock = write_lock!(process);

            plock.push_call_frame(frame);

            for arg in args.iter() {
                plock.add_local(arg.clone());
            }
        }

        let return_val = try!(self.run(process.clone(), code));

        write_lock!(process).pop_call_frame();

        Ok(return_val)
    }

    fn run_file(&self, path_str: &String, process: RcProcess, register: usize) -> EmptyResult {
        {
            let mut executed = write_lock!(self.executed_files);

            if executed.contains(path_str) {
                return Ok(());
            }
            else {
                executed.insert(path_str.clone());
            }
        }

        let mut input_path = PathBuf::from(path_str);

        if input_path.is_relative() {
            let mut found = false;

            for directory in read_lock!(self.directories).iter() {
                let full_path = directory.join(path_str);

                if full_path.exists() {
                    input_path = full_path;
                    found = true;

                    break;
                }
            }

            if !found {
                return Err(format!("No file found for {}", path_str));
            }
        }

        let input_path_str = input_path.to_str().unwrap();

        match bytecode_parser::parse_file(input_path_str) {
            Ok(body) => {
                let self_obj = self.top_level.clone();
                let args = Vec::new();

                let res = try!(
                    self.run_code(process.clone(), body, self_obj, args, None)
                );

                if res.is_some() {
                    write_lock!(process).set_register(register, res.unwrap());
                }

                Ok(())
            },
            Err(err) => {
                Err(format!("Failed to parse {}: {:?}", input_path_str, err))
            }
        }
    }

    fn send_message(&self, name: &String, process: RcProcess,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let receiver_ptr = instruction_object!(instruction, process, 1);
        let allow_private = try!(instruction.arg(3));
        let rest_arg = try!(instruction.arg(4)) == 1;

        let method_ptr = {
            let receiver_ref = receiver_ptr.get();

            try!(
                receiver_ref.get().lookup_method(name)
                    .ok_or_else(|| format!("Undefined method \"{}\" called", name))
            )
        };

        let method_ref = method_ptr.get();
        let method_obj = method_ref.get();

        ensure_compiled_code!(method_obj);

        let method_code = method_obj.value.as_compiled_code();

        if method_code.is_private() && allow_private == 0 {
            return Err(format!("Private method \"{}\" called", name));
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
                let array_ref = last_arg.get();
                let array = array_ref.get();

                ensure_arrays!(array);

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

            let rest_array = write_lock!(process)
                .allocate(object_value::array(rest),
                          self.array_prototype.clone());

            arguments.push(rest_array);
        }
        else if method_code.rest_argument && arguments.len() == 0 {
            let rest_array = write_lock!(process)
                .allocate(object_value::array(Vec::new()),
                          self.array_prototype.clone());

            arguments.push(rest_array);
        }

        if arguments.len() > tot_args && !method_code.rest_argument {
            return Err(format!(
                "{} accepts up to {} arguments, but {} arguments were given",
                name,
                method_code.arguments,
                arguments.len()
            ));
        }

        if arguments.len() < req_args {
            return Err(format!(
                "{} requires {} arguments, but {} arguments were given",
                name,
                method_code.required_arguments,
                arguments.len()
            ));
        }

        let retval = try!(
            self.run_code(process.clone(), method_code, receiver_ptr.clone(),
                          arguments, None)
        );

        if retval.is_some() {
            write_lock!(process).set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn collect_arguments(&self, process: RcProcess, instruction: &Instruction,
                         offset: usize, amount: usize) -> ObjectVecResult {
        let mut args: Vec<ObjectPointer> = Vec::new();

        for index in offset..(offset + amount) {
            let arg_index = try!(instruction.arg(index));
            let arg = try!(read_lock!(process).get_register(arg_index));

            args.push(arg)
        }

        Ok(args)
    }

    fn start_thread(&self, isolated: bool) -> RcThread {
        let self_clone = self.clone();

        let (sender, receiver) = channel();

        let handle = thread::spawn(move || {
            let thread = receiver.recv().unwrap();

            self_clone.run_thread(thread);
        });

        let thread = if isolated {
            self.allocate_isolated_thread(Some(handle))
        }
        else {
            self.allocate_thread(Some(handle))
        };

        sender.send(thread.clone()).unwrap();

        thread
    }

    fn spawn_process(&self, process: RcProcess, code: RcCompiledCode, register: usize, isolated: bool) {
        let (pid, new_proc) = self.allocate_process(code,
                                                    self.top_level.clone());

        if isolated {
            let thread = self.start_thread(true);

            thread.schedule(new_proc);
        }
        else {
            write_lock!(self.threads).schedule(new_proc);
        }

        let mut proc_guard = write_lock!(process);

        let pid_obj = proc_guard.allocate(object_value::integer(pid as i64),
                                          self.integer_prototype.clone());

        proc_guard.set_register(register, pid_obj);
    }

    fn run_thread(&self, thread: RcThread) {
        loop {
            if thread.should_stop() {
                break;
            }

            thread.wait_for_work();

            let process = thread.pop_process();
            let code = read_lock!(process).compiled_code.clone();

            // TODO: process supervision
            match self.run(process.clone(), code) {
                Ok(_) => {
                    write_lock!(self.processes).remove(process);

                    if thread.is_isolated() {
                        break;
                    }
                },
                Err(message) => {
                    self.error(process, message);

                    write_lock!(self.threads).stop();
                }
            }
        }

        write_lock!(self.threads).remove(thread.clone());
    }
}
