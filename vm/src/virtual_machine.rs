//! Virtual Machine for running instructions
//!
//! A VirtualMachine manages threads, runs instructions, starts/terminates
//! threads and so on. VirtualMachine instances are fully self contained
//! allowing multiple instances to run fully isolated in the same process.

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
use instruction::{InstructionType, Instruction};
use memory_manager::{MemoryManager, RcMemoryManager};
use object::RcObject;
use object_value;
use virtual_machine_methods::VirtualMachineMethods;
use virtual_machine_result::*;
use thread::{Thread, RcThread, JoinHandle as ThreadJoinHandle};
use thread_list::ThreadList;

/// A reference counted VirtualMachine.
pub type RcVirtualMachine = Arc<VirtualMachine>;

/// Structure representing a single VM instance.
pub struct VirtualMachine {
    /// The directories to search for bytecode files.
    directories: RwLock<Vec<Box<Path>>>,

    /// All threads that are currently active.
    threads: RwLock<ThreadList>,

    /// The struct for allocating/managing memory.
    memory_manager: RcMemoryManager,

    /// The status of the VM when exiting.
    exit_status: RwLock<Result<(), ()>>,

    /// The files executed by the "run_file" instruction(s)
    executed_files: RwLock<HashSet<String>>
}

impl VirtualMachine {
    pub fn new() -> RcVirtualMachine {
        let vm = VirtualMachine {
            directories: RwLock::new(Vec::new()),
            threads: RwLock::new(ThreadList::new()),
            memory_manager: MemoryManager::new(),
            exit_status: RwLock::new(Ok(())),
            executed_files: RwLock::new(HashSet::new())
        };

        Arc::new(vm)
    }

    fn integer_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).integer_prototype()
    }

    fn float_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).float_prototype()
    }

    fn string_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).string_prototype()
    }

    fn array_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).array_prototype()
    }

    fn thread_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).thread_prototype()
    }

    fn true_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).true_prototype()
    }

    fn false_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).false_prototype()
    }

    fn file_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).file_prototype()
    }

    fn method_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).method_prototype()
    }

    fn binding_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).binding_prototype()
    }

    fn compiled_code_prototype(&self) -> RcObject {
        read_lock!(self.memory_manager).compiled_code_prototype()
    }

    fn false_object(&self) -> RcObject {
        read_lock!(self.memory_manager).false_object()
    }

    fn true_object(&self) -> RcObject {
        read_lock!(self.memory_manager).true_object()
    }

    fn top_level_object(&self) -> RcObject {
        read_lock!(self.memory_manager).top_level_object()
    }

    fn allocate(&self, value: object_value::ObjectValue, prototype: RcObject) -> RcObject {
        write_lock!(self.memory_manager).allocate(value, prototype)
    }

    fn allocate_error(&self, code: u16) -> RcObject {
        write_lock!(self.memory_manager).allocate_error(code)
    }

    fn allocate_thread(&self, code: RcCompiledCode,
                       handle: Option<ThreadJoinHandle>,
                       main_thread: bool) -> RcObject {
        let self_obj  = self.top_level_object();
        let vm_thread = Thread::from_code(code, self_obj, handle);

        if main_thread {
            vm_thread.set_main();
        }

        let thread_obj = write_lock!(self.memory_manager)
            .allocate_thread(vm_thread);

        write_lock!(self.threads).add(thread_obj.clone());

        thread_obj
    }
}

impl VirtualMachineMethods for RcVirtualMachine {
    fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        let thread_obj = self.allocate_thread(code.clone(), None, true);

        self.run_thread(thread_obj, code.clone());

        *read_lock!(self.exit_status)
    }

    fn run(&self, thread: RcThread, code: RcCompiledCode) -> OptionObjectResult {
        if thread.should_stop() {
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
                    run!(self, ins_set_integer, thread, code, instruction);
                },
                InstructionType::SetFloat => {
                    run!(self, ins_set_float, thread, code, instruction);
                },
                InstructionType::SetString => {
                    run!(self, ins_set_string, thread, code, instruction);
                },
                InstructionType::SetObject => {
                    run!(self, ins_set_object, thread, code, instruction);
                },
                InstructionType::SetPrototype => {
                    run!(self, ins_set_prototype, thread, code, instruction);
                },
                InstructionType::GetPrototype => {
                    run!(self, ins_get_prototype, thread, code, instruction);
                },
                InstructionType::SetArray => {
                    run!(self, ins_set_array, thread, code, instruction);
                },
                InstructionType::GetIntegerPrototype => {
                    run!(self, ins_get_integer_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetFloatPrototype => {
                    run!(self, ins_get_float_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetStringPrototype => {
                    run!(self, ins_get_string_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetArrayPrototype => {
                    run!(self, ins_get_array_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetThreadPrototype => {
                    run!(self, ins_get_thread_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetTruePrototype => {
                    run!(self, ins_get_true_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetFalsePrototype => {
                    run!(self, ins_get_false_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetMethodPrototype => {
                    run!(self, ins_get_method_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetCompiledCodePrototype => {
                    run!(self, ins_get_compiled_code_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetBindingPrototype => {
                    run!(self, ins_get_binding_prototype, thread, code,
                         instruction);
                },
                InstructionType::GetTrue => {
                    run!(self, ins_get_true, thread, code, instruction);
                },
                InstructionType::GetFalse => {
                    run!(self, ins_get_false, thread, code, instruction);
                },
                InstructionType::GetBinding => {
                    run!(self, ins_get_binding, thread, code, instruction);
                },
                InstructionType::SetLocal => {
                    run!(self, ins_set_local, thread, code, instruction);
                },
                InstructionType::GetLocal => {
                    run!(self, ins_get_local, thread, code, instruction);
                },
                InstructionType::LocalExists => {
                    run!(self, ins_local_exists, thread, code, instruction);
                },
                InstructionType::SetLiteralConst => {
                    run!(self, ins_set_literal_const, thread, code, instruction);
                },
                InstructionType::SetConst => {
                    run!(self, ins_set_const, thread, code, instruction);
                },
                InstructionType::GetLiteralConst => {
                    run!(self, ins_get_literal_const, thread, code, instruction);
                },
                InstructionType::GetConst => {
                    run!(self, ins_get_const, thread, code, instruction);
                },
                InstructionType::LiteralConstExists => {
                    run!(self, ins_literal_const_exists, thread, code, instruction);
                },
                InstructionType::SetLiteralAttr => {
                    run!(self, ins_set_literal_attr, thread, code, instruction);
                },
                InstructionType::SetAttr => {
                    run!(self, ins_set_attr, thread, code, instruction);
                },
                InstructionType::GetLiteralAttr => {
                    run!(self, ins_get_literal_attr, thread, code, instruction);
                },
                InstructionType::GetAttr => {
                    run!(self, ins_get_attr, thread, code, instruction);
                },
                InstructionType::LiteralAttrExists => {
                    run!(self, ins_literal_attr_exists, thread, code, instruction);
                },
                InstructionType::SetCompiledCode => {
                    run!(self, ins_set_compiled_code, thread, code,
                         instruction);
                },
                InstructionType::SendLiteral => {
                    run!(self, ins_send_literal, thread, code, instruction);
                },
                InstructionType::Send => {
                    run!(self, ins_send, thread, code, instruction);
                },
                InstructionType::LiteralRespondsTo => {
                    run!(self, ins_literal_responds_to, thread, code, instruction);
                },
                InstructionType::RespondsTo => {
                    run!(self, ins_responds_to, thread, code, instruction);
                },
                InstructionType::Return => {
                    retval = run!(self, ins_return, thread, code, instruction);
                },
                InstructionType::GotoIfFalse => {
                    skip_until = run!(self, ins_goto_if_false, thread, code,
                                      instruction);
                },
                InstructionType::GotoIfTrue => {
                    skip_until = run!(self, ins_goto_if_true, thread, code,
                                      instruction);
                },
                InstructionType::Goto => {
                    index = run!(self, ins_goto, thread, code, instruction);
                },
                InstructionType::DefMethod => {
                    run!(self, ins_def_method, thread, code, instruction);
                },
                InstructionType::DefLiteralMethod => {
                    run!(self, ins_def_literal_method, thread, code,
                         instruction);
                },
                InstructionType::RunCode => {
                    run!(self, ins_run_code, thread, code, instruction);
                },
                InstructionType::RunLiteralCode => {
                    run!(self, ins_run_literal_code, thread, code, instruction);
                },
                InstructionType::GetToplevel => {
                    run!(self, ins_get_toplevel, thread, code, instruction);
                },
                InstructionType::GetSelf => {
                    run!(self, ins_get_self, thread, code, instruction);
                },
                InstructionType::IsError => {
                    run!(self, ins_is_error, thread, code, instruction);
                },
                InstructionType::ErrorToString => {
                    run!(self, ins_error_to_integer, thread, code, instruction);
                },
                InstructionType::IntegerAdd => {
                    run!(self, ins_integer_add, thread, code, instruction);
                },
                InstructionType::IntegerDiv => {
                    run!(self, ins_integer_div, thread, code, instruction);
                },
                InstructionType::IntegerMul => {
                    run!(self, ins_integer_mul, thread, code, instruction);
                },
                InstructionType::IntegerSub => {
                    run!(self, ins_integer_sub, thread, code, instruction);
                },
                InstructionType::IntegerMod => {
                    run!(self, ins_integer_mod, thread, code, instruction);
                },
                InstructionType::IntegerToFloat => {
                    run!(self, ins_integer_to_float, thread, code, instruction);
                },
                InstructionType::IntegerToString => {
                    run!(self, ins_integer_to_string, thread, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseAnd => {
                    run!(self, ins_integer_bitwise_and, thread, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseOr => {
                    run!(self, ins_integer_bitwise_or, thread, code,
                         instruction);
                },
                InstructionType::IntegerBitwiseXor => {
                    run!(self, ins_integer_bitwise_xor, thread, code,
                         instruction);
                },
                InstructionType::IntegerShiftLeft => {
                    run!(self, ins_integer_shift_left, thread, code,
                         instruction);
                },
                InstructionType::IntegerShiftRight => {
                    run!(self, ins_integer_shift_right, thread, code,
                         instruction);
                },
                InstructionType::IntegerSmaller => {
                    run!(self, ins_integer_smaller, thread, code, instruction);
                },
                InstructionType::IntegerGreater => {
                    run!(self, ins_integer_greater, thread, code, instruction);
                },
                InstructionType::IntegerEquals => {
                    run!(self, ins_integer_equals, thread, code, instruction);
                },
                InstructionType::StartThread => {
                    run!(self, ins_start_thread, thread, code, instruction);
                },
                InstructionType::FloatAdd => {
                    run!(self, ins_float_add, thread, code, instruction);
                },
                InstructionType::FloatMul => {
                    run!(self, ins_float_mul, thread, code, instruction);
                },
                InstructionType::FloatDiv => {
                    run!(self, ins_float_div, thread, code, instruction);
                },
                InstructionType::FloatSub => {
                    run!(self, ins_float_sub, thread, code, instruction);
                },
                InstructionType::FloatMod => {
                    run!(self, ins_float_mod, thread, code, instruction);
                },
                InstructionType::FloatToInteger => {
                    run!(self, ins_float_to_integer, thread, code, instruction);
                },
                InstructionType::FloatToString => {
                    run!(self, ins_float_to_string, thread, code, instruction);
                },
                InstructionType::FloatSmaller => {
                    run!(self, ins_float_smaller, thread, code, instruction);
                },
                InstructionType::FloatGreater => {
                    run!(self, ins_float_greater, thread, code, instruction);
                },
                InstructionType::FloatEquals => {
                    run!(self, ins_float_equals, thread, code, instruction);
                },
                InstructionType::ArrayInsert => {
                    run!(self, ins_array_insert, thread, code, instruction);
                },
                InstructionType::ArrayAt => {
                    run!(self, ins_array_at, thread, code, instruction);
                },
                InstructionType::ArrayRemove => {
                    run!(self, ins_array_remove, thread, code, instruction);
                },
                InstructionType::ArrayLength => {
                    run!(self, ins_array_length, thread, code, instruction);
                },
                InstructionType::ArrayClear => {
                    run!(self, ins_array_clear, thread, code, instruction);
                },
                InstructionType::StringToLower => {
                    run!(self, ins_string_to_lower, thread, code, instruction);
                },
                InstructionType::StringToUpper => {
                    run!(self, ins_string_to_upper, thread, code, instruction);
                },
                InstructionType::StringEquals => {
                    run!(self, ins_string_equals, thread, code, instruction);
                },
                InstructionType::StringToBytes => {
                    run!(self, ins_string_to_bytes, thread, code, instruction);
                },
                InstructionType::StringFromBytes => {
                    run!(self, ins_string_from_bytes, thread, code, instruction);
                },
                InstructionType::StringLength => {
                    run!(self, ins_string_length, thread, code, instruction);
                },
                InstructionType::StringSize => {
                    run!(self, ins_string_size, thread, code, instruction);
                },
                InstructionType::StdoutWrite => {
                    run!(self, ins_stdout_write, thread, code, instruction);
                },
                InstructionType::StderrWrite => {
                    run!(self, ins_stderr_write, thread, code, instruction);
                },
                InstructionType::StdinRead => {
                    run!(self, ins_stdin_read, thread, code, instruction);
                },
                InstructionType::StdinReadLine => {
                    run!(self, ins_stdin_read_line, thread, code, instruction);
                },
                InstructionType::FileOpen => {
                    run!(self, ins_file_open, thread, code, instruction);
                },
                InstructionType::FileWrite => {
                    run!(self, ins_file_write, thread, code, instruction);
                },
                InstructionType::FileRead => {
                    run!(self, ins_file_read, thread, code, instruction);
                },
                InstructionType::FileReadLine => {
                    run!(self, ins_file_read_line, thread, code, instruction);
                },
                InstructionType::FileFlush => {
                    run!(self, ins_file_flush, thread, code, instruction);
                },
                InstructionType::FileSize => {
                    run!(self, ins_file_size, thread, code, instruction);
                },
                InstructionType::FileSeek => {
                    run!(self, ins_file_seek, thread, code, instruction);
                },
                InstructionType::RunLiteralFile => {
                    run!(self, ins_run_literal_file, thread, code, instruction);
                },
                InstructionType::RunFile => {
                    run!(self, ins_run_file, thread, code, instruction);
                },
                InstructionType::GetCaller => {
                    run!(self, ins_get_caller, thread, code, instruction);
                },
            };
        }

        Ok(retval)
    }

    fn ins_set_integer(&self, thread: RcThread, code: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index    = try!(instruction.arg(1));
        let value    = *try!(code.integer(index));

        let obj = self.allocate(object_value::integer(value),
                                self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_float(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index    = try!(instruction.arg(1));
        let value    = *try!(code.float(index));

        let obj = self.allocate(object_value::float(value),
                                self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_string(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index    = try!(instruction.arg(1));
        let value    = try!(code.string(index));

        let obj = self.allocate(object_value::string(value.clone()),
                                self.string_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_object(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        let proto_index_res = instruction.arg(1);

        let obj = write_lock!(self.memory_manager)
            .new_object(object_value::none());

        if proto_index_res.is_ok() {
            let proto_index = proto_index_res.unwrap();
            let proto       = try!(thread.get_register(proto_index));

            write_lock!(obj).set_prototype(proto);
        }

        write_lock!(self.memory_manager)
            .allocate_prepared(obj.clone());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_prototype(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let source = instruction_object!(instruction, thread, 0);
        let proto  = instruction_object!(instruction, thread, 1);

        write_lock!(source).set_prototype(proto);

        Ok(())
    }

    fn ins_get_prototype(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let source     = instruction_object!(instruction, thread, 1);
        let source_obj = read_lock!(source);

        let proto = try!(source_obj.prototype().ok_or(format!(
            "The object in register {} does not have a prototype",
            instruction.arguments[1]
        )));

        thread.set_register(register, proto);

        Ok(())
    }

    fn ins_set_array(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register  = try!(instruction.arg(0));
        let val_count = instruction.arguments.len() - 1;

        let values = try!(
            self.collect_arguments(thread.clone(), instruction, 1, val_count)
        );

        let obj = self.allocate(object_value::array(values),
                                self.array_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_get_integer_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.integer_prototype());

        Ok(())
    }

    fn ins_get_float_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.float_prototype());

        Ok(())
    }

    fn ins_get_string_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.string_prototype());

        Ok(())
    }

    fn ins_get_array_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.array_prototype());

        Ok(())
    }

    fn ins_get_thread_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.thread_prototype());

        Ok(())
    }

    fn ins_get_true_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.true_prototype());

        Ok(())
    }

    fn ins_get_false_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.false_prototype());

        Ok(())
    }

    fn ins_get_method_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.method_prototype());

        Ok(())
    }

    fn ins_get_binding_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.binding_prototype());

        Ok(())
    }

    fn ins_get_compiled_code_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.compiled_code_prototype());

        Ok(())
    }

    fn ins_get_true(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.true_object());

        Ok(())
    }

    fn ins_get_false(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.false_object());

        Ok(())
    }

    fn ins_get_binding(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let frame    = read_lock!(thread.call_frame);

        let obj = self.allocate(object_value::binding(frame.binding.clone()),
                                self.binding_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let local_index = try!(instruction.arg(0));
        let object      = instruction_object!(instruction, thread, 1);

        thread.set_local(local_index, object);

        Ok(())
    }

    fn ins_get_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let local_index = try!(instruction.arg(1));

        let object = try!(thread.get_local(local_index));

        thread.set_register(register, object);

        Ok(())
    }

    fn ins_local_exists(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let local_index = try!(instruction.arg(1));

        let value = if thread.local_exists(local_index) {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, value);

        Ok(())
    }

    fn ins_set_literal_const(&self, thread: RcThread, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let target     = instruction_object!(instruction, thread, 0);
        let name_index = try!(instruction.arg(1));
        let source     = instruction_object!(instruction, thread, 2);
        let name       = try!(code.string(name_index));

        write_lock!(target).add_constant(name.clone(), source);

        Ok(())
    }

    fn ins_set_const(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let target = instruction_object!(instruction, thread, 0);
        let name   = instruction_object!(instruction, thread, 1);
        let source = instruction_object!(instruction, thread, 2);

        let name_obj = read_lock!(name);

        ensure_strings!(name_obj);

        let name_str = name_obj.value.as_string().clone();

        write_lock!(target).add_constant(name_str, source);

        Ok(())
    }

    fn ins_get_literal_const(&self, thread: RcThread, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let src        = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));
        let name       = try!(code.string(name_index));

        let object = try!(
            read_lock!(src).lookup_constant(name)
                .ok_or(constant_error!(instruction.arguments[1], name))
        );

        thread.set_register(register, object);

        Ok(())
    }

    fn ins_get_const(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let src      = instruction_object!(instruction, thread, 1);
        let name     = instruction_object!(instruction, thread, 2);

        let name_obj = read_lock!(name);

        ensure_strings!(name_obj);

        let name_str = name_obj.value.as_string();

        let object = try!(
            read_lock!(src).lookup_constant(name_str)
                .ok_or(constant_error!(instruction.arguments[1], name_str))
        );

        thread.set_register(register, object);

        Ok(())
    }

    fn ins_literal_const_exists(&self, thread: RcThread, code: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let source     = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));
        let name       = try!(code.string(name_index));
        let source_obj = read_lock!(source);

        let constant = source_obj.lookup_constant(name);

        if constant.is_some() {
            thread.set_register(register, self.true_object());
        }
        else {
            thread.set_register(register, self.false_object());
        }

        Ok(())
    }

    fn ins_set_literal_attr(&self, thread: RcThread, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let target_object = instruction_object!(instruction, thread, 0);
        let name_index    = try!(instruction.arg(1));
        let source_object = instruction_object!(instruction, thread, 2);

        let name = try!(code.string(name_index));

        write_lock!(target_object)
            .add_attribute(name.clone(), source_object);

        Ok(())
    }

    fn ins_set_attr(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let target_object = instruction_object!(instruction, thread, 0);
        let name_lock     = instruction_object!(instruction, thread, 1);
        let source_object = instruction_object!(instruction, thread, 2);

        let name_obj = read_lock!(name_lock);

        ensure_strings!(name_obj);

        let name = name_obj.value.as_string();

        write_lock!(target_object)
            .add_attribute(name.clone(), source_object);

        Ok(())
    }

    fn ins_get_literal_attr(&self, thread: RcThread, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let source     = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));

        let name = try!(code.string(name_index));

        let attr = try!(
            read_lock!(source).lookup_attribute(name)
                .ok_or(attribute_error!(instruction.arguments[1], name))
        );

        thread.set_register(register, attr);

        Ok(())
    }

    fn ins_get_attr(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register  = try!(instruction.arg(0));
        let source    = instruction_object!(instruction, thread, 1);
        let name_lock = instruction_object!(instruction, thread, 2);

        let name_obj = read_lock!(name_lock);

        ensure_strings!(name_obj);

        let name = name_obj.value.as_string();

        let attr = try!(
            read_lock!(source).lookup_attribute(name)
                .ok_or(attribute_error!(instruction.arguments[1], name))
        );

        thread.set_register(register, attr);

        Ok(())
    }

    fn ins_literal_attr_exists(&self, thread: RcThread, code: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let obj = if read_lock!(source).has_attribute(name) {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_set_compiled_code(&self, thread: RcThread, code: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let cc_index = try!(instruction.arg(1));

        let cc = try!(code.code_object(cc_index));

        let obj = self.allocate(object_value::compiled_code(cc),
                                self.compiled_code_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_send_literal(&self, thread: RcThread, code: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let name_index = try!(instruction.arg(2));
        let name       = try!(code.string(name_index));

        self.send_message(name, thread, instruction)
    }

    fn ins_send(&self, thread: RcThread, _: RcCompiledCode,
                instruction: &Instruction) -> EmptyResult {
        let lock   = instruction_object!(instruction, thread, 2);
        let string = read_lock!(lock);

        ensure_strings!(string);

        self.send_message(string.value.as_string(), thread, instruction)
    }

    fn ins_literal_responds_to(&self, thread: RcThread, code: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));
        let name = try!(code.string(name_index));

        let source_obj = read_lock!(source);

        let result = if source_obj.responds_to(name) {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_responds_to(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let source = instruction_object!(instruction, thread, 1);
        let name = instruction_object!(instruction, thread, 2);

        let name_obj = read_lock!(name);
        let source_obj = read_lock!(source);

        ensure_strings!(name_obj);

        let result = if source_obj.responds_to(name_obj.value.as_string()) {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_return(&self, thread: RcThread, _: RcCompiledCode,
                  instruction: &Instruction) -> OptionObjectResult {
        let register = try!(instruction.arg(0));

        Ok(thread.get_register_option(register))
    }

    fn ins_goto_if_false(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> OptionIntegerResult {
        let go_to     = try!(instruction.arg(0));
        let value_reg = try!(instruction.arg(1));
        let value     = thread.get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if read_lock!(obj).truthy() {
                    None
                }
                else {
                    Some(go_to)
                }
            },
            None => { Some(go_to) }
        };

        Ok(matched)
    }

    fn ins_goto_if_true(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> OptionIntegerResult {
        let go_to     = try!(instruction.arg(0));
        let value_reg = try!(instruction.arg(1));
        let value     = thread.get_register_option(value_reg);

        let matched = match value {
            Some(obj) => {
                if read_lock!(obj).truthy() {
                    Some(go_to)
                }
                else {
                    None
                }
            },
            None => { None }
        };

        Ok(matched)
    }

    fn ins_goto(&self, _: RcThread, _: RcCompiledCode,
                instruction: &Instruction) -> IntegerResult {
        let go_to = try!(instruction.arg(0));

        Ok(go_to)
    }

    fn ins_def_method(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let receiver_lock = instruction_object!(instruction, thread, 0);
        let name_lock     = instruction_object!(instruction, thread, 1);
        let cc_lock       = instruction_object!(instruction, thread, 2);

        let mut receiver = write_lock!(receiver_lock);
        let name_obj     = read_lock!(name_lock);
        let cc_obj       = read_lock!(cc_lock);

        ensure_strings!(name_obj);
        ensure_compiled_code!(cc_obj);

        let name = name_obj.value.as_string();
        let cc   = cc_obj.value.as_compiled_code();

        let method = self.allocate(object_value::compiled_code(cc),
                                   self.method_prototype());

        receiver.add_method(name.clone(), method);

        Ok(())
    }

    fn ins_def_literal_method(&self, thread: RcThread, code: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let receiver_lock = instruction_object!(instruction, thread, 0);
        let name_index    = try!(instruction.arg(1));
        let cc_index      = try!(instruction.arg(2));

        let name = try!(code.string(name_index));
        let cc   = try!(code.code_object(cc_index));

        let mut receiver = write_lock!(receiver_lock);

        let method = self.allocate(object_value::compiled_code(cc),
                                   self.method_prototype());

        receiver.add_method(name.clone(), method);

        Ok(())
    }

    fn ins_run_code(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let cc_lock  = instruction_object!(instruction, thread, 1);
        let arg_lock = instruction_object!(instruction, thread, 2);

        let cc_obj  = read_lock!(cc_lock);
        let arg_obj = read_lock!(arg_lock);

        ensure_compiled_code!(cc_obj);
        ensure_integers!(arg_obj);

        let arg_count = arg_obj.value.as_integer() as usize;
        let code_obj  = cc_obj.value.as_compiled_code();

        let arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 3, arg_count)
        );

        let bidx = 3 + arg_count;

        let binding = if let Some(obj) = thread.get_register_option(bidx) {
            let lock = read_lock!(obj);

            if !lock.value.is_binding() {
                return Err(format!("Argument {} is not a valid Binding", bidx));
            }

            Some(lock.value.as_binding())
        }
        else {
            None
        };

        let retval = try!(
            self.run_code(thread.clone(), code_obj, cc_lock.clone(), arguments,
                          binding)
        );

        if retval.is_some() {
            thread.set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn ins_run_literal_code(&self, thread: RcThread, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let code_index = try!(instruction.arg(1));
        let receiver   = instruction_object!(instruction, thread, 2);
        let code_obj   = try!(code.code_object(code_index));

        let retval = try!(
            self.run_code(thread.clone(), code_obj, receiver, Vec::new(), None)
        );

        if retval.is_some() {
            thread.set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn ins_get_toplevel(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        thread.set_register(register, self.top_level_object());

        Ok(())
    }

    fn ins_get_self(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        let self_object = read_lock!(thread.call_frame).self_object();

        thread.set_register(register, self_object);

        Ok(())
    }

    fn ins_is_error(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let obj_lock = instruction_object!(instruction, thread, 1);
        let obj      = read_lock!(obj_lock);

        let result = if obj.value.is_error() {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_error_to_integer(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let error_lock = instruction_object!(instruction, thread, 1);
        let error      = read_lock!(error_lock);

        let proto   = self.integer_prototype();
        let integer = error.value.as_error() as i64;
        let result  = self.allocate(object_value::integer(integer), proto);

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_integer_add(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() + arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_div(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() / arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_mul(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() * arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_sub(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() - arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_mod(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() % arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_to_float(&self, thread: RcThread, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register     = try!(instruction.arg(0));
        let integer_lock = instruction_object!(instruction, thread, 1);
        let integer      = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer() as f64;
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_to_string(&self, thread: RcThread, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register     = try!(instruction.arg(0));
        let integer_lock = instruction_object!(instruction, thread, 1);

        let integer = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer().to_string();
        let obj    = self.allocate(object_value::string(result),
                                   self.string_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_and(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() & arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_or(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() | arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_bitwise_xor(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() ^ arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_shift_left(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() << arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_shift_right(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() >> arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_integer_smaller(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() < arg.value.as_integer();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_integer_greater(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() > arg.value.as_integer();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_integer_equals(&self, thread: RcThread, _: RcCompiledCode,
                          instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() == arg.value.as_integer();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_start_thread(&self, thread: RcThread, code: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let code_index  = try!(instruction.arg(1));
        let thread_code = try!(code.code_object(code_index));

        let thread_object = self.start_thread(thread_code);

        thread.set_register(register, thread_object);

        Ok(())
    }

    fn ins_float_add(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let added = receiver.value.as_float() + arg.value.as_float();
        let obj   = self.allocate(object_value::float(added),
                                  self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_mul(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() * arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_div(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() / arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_sub(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() - arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_mod(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() % arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_to_integer(&self, thread: RcThread, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let float_lock = instruction_object!(instruction, thread, 1);
        let float      = read_lock!(float_lock);

        ensure_floats!(float);

        let result = float.value.as_float() as i64;
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_to_string(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let float_lock = instruction_object!(instruction, thread, 1);
        let float      = read_lock!(float_lock);

        ensure_floats!(float);

        let result = float.value.as_float().to_string();
        let obj    = self.allocate(object_value::string(result),
                                   self.string_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_float_smaller(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() < arg.value.as_float();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_float_greater(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() > arg.value.as_float();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_float_equals(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() == arg.value.as_float();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_array_insert(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let array_lock = instruction_object!(instruction, thread, 0);
        let index      = try!(instruction.arg(1));
        let value_lock = instruction_object!(instruction, thread, 2);
        let mut array  = write_lock!(array_lock);

        ensure_arrays!(array);

        let mut vector = array.value.as_array_mut();

        ensure_array_within_bounds!(vector, index);

        vector.insert(index, value_lock);

        Ok(())
    }

    fn ins_array_at(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let index      = try!(instruction.arg(2));
        let array      = read_lock!(array_lock);

        ensure_arrays!(array);

        let vector = array.value.as_array();

        ensure_array_within_bounds!(vector, index);

        let value = vector[index].clone();

        thread.set_register(register, value);

        Ok(())
    }

    fn ins_array_remove(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let index      = try!(instruction.arg(1));
        let mut array  = write_lock!(array_lock);

        ensure_arrays!(array);

        let mut vector = array.value.as_array_mut();

        ensure_array_within_bounds!(vector, index);

        let value = vector.remove(index);

        thread.set_register(register, value);

        Ok(())
    }

    fn ins_array_length(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register   = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let array      = read_lock!(array_lock);

        ensure_arrays!(array);

        let vector = array.value.as_array();
        let length = vector.len() as i64;

        let obj = self.allocate(object_value::integer(length),
                                self.integer_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_array_clear(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let array_lock = instruction_object!(instruction, thread, 0);
        let mut array  = write_lock!(array_lock);

        ensure_arrays!(array);

        let mut vector = array.value.as_array_mut();

        vector.clear();

        Ok(())
    }

    fn ins_string_to_lower(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let source_lock = instruction_object!(instruction, thread, 1);
        let source      = read_lock!(source_lock);

        ensure_strings!(source);

        let lower = source.value.as_string().to_lowercase();
        let obj   = self.allocate(object_value::string(lower),
                                  self.string_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_string_to_upper(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let source_lock = instruction_object!(instruction, thread, 1);
        let source      = read_lock!(source_lock);

        ensure_strings!(source);

        let upper = source.value.as_string().to_uppercase();
        let obj   = self.allocate(object_value::string(upper),
                                  self.string_prototype());

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_string_equals(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(receiver, arg);

        let result = receiver.value.as_string() == arg.value.as_string();

        let boolean = if result {
            self.true_object()
        }
        else {
            self.false_object()
        };

        thread.set_register(register, boolean);

        Ok(())
    }

    fn ins_string_to_bytes(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto   = self.integer_prototype();
        let array_proto = self.array_prototype();

        let array = arg.value.as_string().as_bytes().iter().map(|&b| {
            self.allocate(object_value::integer(b as i64), int_proto.clone())
        }).collect::<Vec<_>>();

        let obj = self.allocate(object_value::array(array), array_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_string_from_bytes(&self, thread: RcThread, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_arrays!(arg);

        let string_proto = self.string_prototype();
        let array        = arg.value.as_array();

        for int_lock in array.iter() {
            let int = read_lock!(int_lock);

            ensure_integers!(int);
        }

        let bytes = arg.value.as_array().iter().map(|ref int_lock| {
            read_lock!(int_lock).value.as_integer() as u8
        }).collect::<Vec<_>>();

        let string = try_error!(try_from_utf8!(bytes), self, thread, register);
        let obj    = self.allocate(object_value::string(string), string_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_string_length(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto = self.integer_prototype();

        let length = arg.value.as_string().chars().count() as i64;
        let obj    = self.allocate(object_value::integer(length), int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_string_size(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto = self.integer_prototype();

        let size = arg.value.as_string().len() as i64;
        let obj  = self.allocate(object_value::integer(size), int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_stdout_write(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto  = self.integer_prototype();
        let mut stdout = io::stdout();

        let result = try_io!(stdout.write(arg.value.as_string().as_bytes()),
                             self, thread, register);

        try_io!(stdout.flush(), self, thread, register);

        let obj = self.allocate(object_value::integer(result as i64),
                                int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_stderr_write(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto  = self.integer_prototype();
        let mut stderr = io::stderr();

        let result = try_io!(stderr.write(arg.value.as_string().as_bytes()),
                             self, thread, register);

        try_io!(stderr.flush(), self, thread, register);

        let obj = self.allocate(object_value::integer(result as i64),
                                int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_stdin_read(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let proto    = self.string_prototype();

        let mut buffer = file_reading_buffer!(instruction, thread, 1);

        try_io!(io::stdin().read_to_string(&mut buffer), self, thread, register);

        let obj = self.allocate(object_value::string(buffer), proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_stdin_read_line(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let proto    = self.string_prototype();

        let mut buffer = String::new();

        try_io!(io::stdin().read_line(&mut buffer), self, thread, register);

        let obj = self.allocate(object_value::string(buffer), proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_file_open(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register  = try!(instruction.arg(0));
        let path_lock = instruction_object!(instruction, thread, 1);
        let mode_lock = instruction_object!(instruction, thread, 2);

        let file_proto = self.file_prototype();

        let path = read_lock!(path_lock);
        let mode = read_lock!(mode_lock);

        let path_string   = path.value.as_string();
        let mode_string   = mode.value.as_string().as_ref();
        let mut open_opts = OpenOptions::new();

        match mode_string {
            "r"  => open_opts.read(true),
            "r+" => open_opts.read(true).write(true).truncate(true).create(true),
            "w"  => open_opts.write(true).truncate(true).create(true),
            "w+" => open_opts.read(true).write(true).truncate(true).create(true),
            "a"  => open_opts.append(true).create(true),
            "a+" => open_opts.read(true).append(true).create(true),
            _    => set_error!(errors::IO_INVALID_OPEN_MODE, self, thread, register)
        };

        let file = try_io!(open_opts.open(path_string), self, thread, register);
        let obj  = self.allocate(object_value::file(file), file_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_file_write(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let file_lock   = instruction_object!(instruction, thread, 1);
        let string_lock = instruction_object!(instruction, thread, 2);

        let mut file = write_lock!(file_lock);
        let string   = read_lock!(string_lock);

        ensure_files!(file);
        ensure_strings!(string);

        let int_proto = self.integer_prototype();
        let mut file  = file.value.as_file_mut();
        let bytes     = string.value.as_string().as_bytes();

        let result = try_io!(file.write(bytes), self, thread, register);

        let obj = self.allocate(object_value::integer(result as i64),
                                int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_file_read(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register     = try!(instruction.arg(0));
        let file_lock    = instruction_object!(instruction, thread, 1);
        let mut file_obj = write_lock!(file_lock);

        ensure_files!(file_obj);

        let mut buffer = file_reading_buffer!(instruction, thread, 2);
        let int_proto  = self.integer_prototype();
        let mut file   = file_obj.value.as_file_mut();

        try_io!(file.read_to_string(&mut buffer), self, thread, register);

        let obj = self.allocate(object_value::string(buffer), int_proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_file_read_line(&self, thread: RcThread, _: RcCompiledCode,
                          instruction: &Instruction) -> EmptyResult {
        let register     = try!(instruction.arg(0));
        let file_lock    = instruction_object!(instruction, thread, 1);
        let mut file_obj = write_lock!(file_lock);

        ensure_files!(file_obj);

        let proto     = self.string_prototype();
        let mut file  = file_obj.value.as_file_mut();
        let mut bytes = Vec::new();

        for result in file.bytes() {
            let byte = try_io!(result, self, thread, register);

            bytes.push(byte);

            if byte == 0xA {
                break;
            }
        }

        let string = try_error!(try_from_utf8!(bytes), self, thread, register);
        let obj    = self.allocate(object_value::string(string), proto);

        thread.set_register(register, obj);

        Ok(())
    }

    fn ins_file_flush(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register     = try!(instruction.arg(0));
        let file_lock    = instruction_object!(instruction, thread, 1);
        let mut file_obj = write_lock!(file_lock);

        ensure_files!(file_obj);

        let mut file = file_obj.value.as_file_mut();

        try_io!(file.flush(), self, thread, register);

        thread.set_register(register, self.true_object());

        Ok(())
    }

    fn ins_file_size(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register  = try!(instruction.arg(0));
        let file_lock = instruction_object!(instruction, thread, 1);
        let file_obj  = read_lock!(file_lock);

        ensure_files!(file_obj);

        let file = file_obj.value.as_file();
        let meta = try_io!(file.metadata(), self, thread, register);

        let size   = meta.len() as i64;
        let proto  = self.integer_prototype();
        let result = self.allocate(object_value::integer(size), proto);

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_file_seek(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let register    = try!(instruction.arg(0));
        let file_lock   = instruction_object!(instruction, thread, 1);
        let offset_lock = instruction_object!(instruction, thread, 2);

        let mut file_obj = write_lock!(file_lock);
        let offset_obj   = read_lock!(offset_lock);

        ensure_files!(file_obj);
        ensure_integers!(offset_obj);

        let mut file = file_obj.value.as_file_mut();
        let offset   = offset_obj.value.as_integer();

        ensure_positive_read_size!(offset);

        let seek_from  = SeekFrom::Start(offset as u64);
        let new_offset = try_io!(file.seek(seek_from), self, thread, register);

        let proto  = self.integer_prototype();
        let result = self.allocate(object_value::integer(new_offset as i64),
                                   proto);

        thread.set_register(register, result);

        Ok(())
    }

    fn ins_run_literal_file(&self, thread: RcThread, code: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let index    = try!(instruction.arg(1));
        let path     = try!(code.string(index));

        self.run_file(path, thread, register)
    }

    fn ins_run_file(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));
        let lock     = instruction_object!(instruction, thread, 1);
        let string   = read_lock!(lock);

        ensure_strings!(string);

        self.run_file(string.value.as_string(), thread, register)
    }

    fn ins_get_caller(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let register = try!(instruction.arg(0));

        let caller = {
            let frame = read_lock!(thread.call_frame);

            if let Some(parent) = frame.parent() {
                parent.self_object()
            }
            else {
                frame.self_object()
            }
        };

        thread.set_register(register, caller);

        Ok(())
    }

    fn error(&self, thread: RcThread, message: String) {
        let mut stderr = io::stderr();
        let mut error  = message.to_string();
        let frame      = read_lock!(thread.call_frame);

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
                thread: RcThread,
                code: RcCompiledCode,
                self_obj: RcObject,
                args: Vec<RcObject>,
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

            thread.push_call_frame(frame);

            for arg in args.iter() {
                thread.add_local(arg.clone());
            }
        }

        let return_val = try!(self.run(thread.clone(), code));

        thread.pop_call_frame();

        Ok(return_val)
    }

    fn run_file(&self, path_str: &String, thread: RcThread, register: usize) -> EmptyResult {
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
                let self_obj = self.top_level_object();
                let args     = Vec::new();

                let res = try!(
                    self.run_code(thread.clone(), body, self_obj, args, None)
                );

                if res.is_some() {
                    thread.set_register(register, res.unwrap());
                }

                Ok(())
            },
            Err(err) => {
                Err(format!("Failed to parse {}: {:?}", input_path_str, err))
            }
        }
    }

    fn send_message(&self, name: &String, thread: RcThread,
                    instruction: &Instruction) -> EmptyResult {
        let register      = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let allow_private = try!(instruction.arg(3));
        let rest_arg      = try!(instruction.arg(4)) == 1;

        let method_lock = try!(
            read_lock!(receiver_lock).lookup_method(name)
                .ok_or(format!("Undefined method \"{}\" called", name))
        );

        let method_obj = read_lock!(method_lock);

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
            self.collect_arguments(thread.clone(), instruction, 5, arg_count)
        );

        // Unpack the last argument if it's a rest argument
        if rest_arg {
            if let Some(last_arg) = arguments.pop() {
                let array = read_lock!(last_arg);

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

            let rest_array = self.allocate(object_value::array(rest),
                                           self.array_prototype());

            arguments.push(rest_array);
        }
        else if method_code.rest_argument && arguments.len() == 0 {
            let rest_array = self.allocate(object_value::array(Vec::new()),
                                           self.array_prototype());

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
            self.run_code(thread.clone(), method_code, receiver_lock.clone(),
                          arguments, None)
        );

        if retval.is_some() {
            thread.set_register(register, retval.unwrap());
        }

        Ok(())
    }

    fn collect_arguments(&self, thread: RcThread, instruction: &Instruction,
                         offset: usize, amount: usize) -> ObjectVecResult {
        let mut args: Vec<RcObject> = Vec::new();

        for index in offset..(offset + amount) {
            let arg_index = try!(instruction.arg(index));
            let arg       = try!(thread.get_register(arg_index));

            args.push(arg)
        }

        Ok(args)
    }

    fn start_thread(&self, code: RcCompiledCode) -> RcObject {
        let self_clone = self.clone();
        let code_clone = code.clone();

        let (chan_sender, chan_receiver) = channel();

        let handle = thread::spawn(move || {
            let thread_obj: RcObject = chan_receiver.recv().unwrap();

            self_clone.run_thread(thread_obj, code_clone);
        });

        let thread_obj = self.allocate_thread(code, Some(handle), false);

        chan_sender.send(thread_obj.clone()).unwrap();

        thread_obj
    }

    fn run_thread(&self, thread: RcObject, code: RcCompiledCode) {
        let vm_thread = read_lock!(thread).value.as_thread();
        let result    = self.run(vm_thread.clone(), code);

        write_lock!(self.threads).remove(thread.clone());

        write_lock!(thread).unpin();

        match result {
            Ok(obj) => {
                vm_thread.set_value(obj);
            },
            Err(message) => {
                self.error(vm_thread, message);

                write_lock!(self.threads).stop();
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use virtual_machine_methods::*;
    use call_frame::CallFrame;
    use compiled_code::CompiledCode;
    use instruction::{Instruction, InstructionType};
    use object::Object;
    use object_value;
    use thread::Thread;

    macro_rules! compiled_code {
        ($ins: expr) => (
            CompiledCode::new("test".to_string(), "test".to_string(), 1, $ins)
        );
    }

    macro_rules! call_frame {
        () => ({
            let self_obj = Object::new(1, object_value::none());

            CallFrame::new("foo".to_string(), "foo".to_string(), 1, self_obj)
        });
    }

    macro_rules! instruction {
        ($ins_type: expr, $args: expr) => (
            Instruction::new($ins_type, $args, 1, 1)
        );
    }

    macro_rules! run {
        ($vm: ident, $thread: expr, $cc: expr) => (
            $vm.run($thread.clone(), Arc::new($cc))
        );
    }

    // TODO: test for start()
    // TODO: test for run()

    #[test]
    fn test_ins_set_integer_without_arguments() {
        let vm = VirtualMachine::new();
        let cc = compiled_code!(
            vec![instruction!(InstructionType::SetInteger, Vec::new())]
        );

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        assert!(result.is_err());
    }

    #[test]
    fn test_ins_set_integer_without_literal_index() {
        let vm = VirtualMachine::new();
        let cc = compiled_code!(
            vec![instruction!(InstructionType::SetInteger, vec![0])]
        );

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        assert!(result.is_err());
    }

    #[test]
    fn test_ins_set_integer_with_undefined_literal() {
        let vm = VirtualMachine::new();
        let cc = compiled_code!(
            vec![instruction!(InstructionType::SetInteger, vec![0, 0])]
        );

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        assert!(result.is_err());
    }

    #[test]
    fn test_ins_set_integer_with_valid_arguments() {
        let vm = VirtualMachine::new();

        let mut cc = compiled_code!(
            vec![instruction!(InstructionType::SetInteger, vec![1, 0])]
        );

        cc.integer_literals.push(10);

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        let int_obj = thread.get_register(1).unwrap();
        let value   = read_lock!(int_obj).value.as_integer();

        assert!(result.is_ok());

        assert_eq!(value, 10);
    }
}
