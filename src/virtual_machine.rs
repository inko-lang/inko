//! Virtual Machine for running instructions
//!
//! A VirtualMachine manages threads, runs instructions, starts/terminates
//! threads and so on. VirtualMachine instances are fully self contained
//! allowing multiple instances to run fully isolated in the same process.

use std::error::Error;
use std::io::{self, Write, Read};
use std::thread;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use instruction::{InstructionType, Instruction};
use memory_manager::{MemoryManager, RcMemoryManager};
use object::RcObject;
use object_value;
use open_options;
use virtual_machine_methods::VirtualMachineMethods;
use virtual_machine_result::*;
use thread::{Thread, RcThread};
use thread_list::ThreadList;

/// A reference counted VirtualMachine.
pub type RcVirtualMachine = Arc<VirtualMachine>;

/// Structure representing a single VM instance.
pub struct VirtualMachine {
    // All threads that are currently active.
    threads: RwLock<ThreadList>,

    // The struct for allocating/managing memory.
    memory_manager: RcMemoryManager,

    // The status of the VM when exiting.
    exit_status: RwLock<Result<(), ()>>
}

impl VirtualMachine {
    pub fn new() -> RcVirtualMachine {
        let vm = VirtualMachine {
            threads: RwLock::new(ThreadList::new()),
            memory_manager: MemoryManager::new(),
            exit_status: RwLock::new(Ok(()))
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

    fn false_object(&self) -> RcObject {
        read_lock!(self.memory_manager).false_object()
    }

    fn true_object(&self) -> RcObject {
        read_lock!(self.memory_manager).true_object()
    }

    fn allocate(&self, value: object_value::ObjectValue, prototype: RcObject) -> RcObject {
        write_lock!(self.memory_manager).allocate(value, prototype)
    }
}

impl VirtualMachineMethods for RcVirtualMachine {
    fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        let thread_obj = self.run_thread(code, true);
        let vm_thread  = write_lock!(thread_obj).value.as_thread();
        let handle     = vm_thread.take_join_handle();

        if handle.is_some() {
            handle.unwrap().join().unwrap();
        }

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
                    continue;
                }
                else {
                    skip_until = None;
                }
            }

            // Incremented _before_ the instructions so that the "goto"
            // instruction can overwrite it.
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
                InstructionType::SetArray => {
                    run!(self, ins_set_array, thread, code, instruction);
                },
                InstructionType::SetName => {
                    run!(self, ins_set_name, thread, code, instruction);
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
                InstructionType::SetTrue => {
                    run!(self, ins_set_true, thread, code, instruction);
                },
                InstructionType::SetFalse => {
                    run!(self, ins_set_false, thread, code, instruction);
                },
                InstructionType::SetLocal => {
                    run!(self, ins_set_local, thread, code, instruction);
                },
                InstructionType::GetLocal => {
                    run!(self, ins_get_local, thread, code, instruction);
                },
                InstructionType::SetConst => {
                    run!(self, ins_set_const, thread, code, instruction);
                },
                InstructionType::GetConst => {
                    run!(self, ins_get_const, thread, code, instruction);
                },
                InstructionType::SetAttr => {
                    run!(self, ins_set_attr, thread, code, instruction);
                },
                InstructionType::GetAttr => {
                    run!(self, ins_get_attr, thread, code, instruction);
                },
                InstructionType::Send => {
                    run!(self, ins_send, thread, code, instruction);
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
                InstructionType::RunCode => {
                    run!(self, ins_run_code, thread, code, instruction);
                },
                InstructionType::GetToplevel => {
                    run!(self, ins_get_toplevel, thread, code, instruction);
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
                }
            };
        }

        Ok(retval)
    }

    fn ins_set_integer(&self, thread: RcThread, code: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot  = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let value = *try!(code.integer(index));

        let obj = self.allocate(object_value::integer(value),
                                self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_float(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot  = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let value = *try!(code.float(index));

        let obj = self.allocate(object_value::float(value),
                                self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_string(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let slot  = try!(instruction.arg(0));
        let index = try!(instruction.arg(1));
        let value = try!(code.string(index));

        let obj = self.allocate(object_value::string(value.clone()),
                                self.string_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_object(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        let proto_index_opt = instruction.arguments.get(1);

        let obj = write_lock!(self.memory_manager)
            .new_object(object_value::none());

        if proto_index_opt.is_some() {
            let proto_index = *proto_index_opt.unwrap();
            let proto       = try!(thread.get_register(proto_index));

            write_lock!(obj).set_prototype(proto);
        }

        write_lock!(self.memory_manager)
            .allocate_prepared(obj.clone());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_array(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot      = try!(instruction.arg(0));
        let val_count = try!(instruction.arg(1));

        let values = try!(
            self.collect_arguments(thread.clone(), instruction, 2, val_count)
        );

        let obj = self.allocate(object_value::array(values),
                                self.array_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_name(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let name_index = try!(instruction.arg(1));

        let obj  = instruction_object!(instruction, thread, 0);
        let name = try!(code.string(name_index));

        write_lock!(obj).set_name(name.clone());

        Ok(())
    }

    fn ins_get_integer_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                 instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.integer_prototype());

        Ok(())
    }

    fn ins_get_float_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.float_prototype());

        Ok(())
    }

    fn ins_get_string_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.string_prototype());

        Ok(())
    }

    fn ins_get_array_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.array_prototype());

        Ok(())
    }

    fn ins_get_thread_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.thread_prototype());

        Ok(())
    }

    fn ins_get_true_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.true_prototype());

        Ok(())
    }

    fn ins_get_false_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.false_prototype());

        Ok(())
    }

    fn ins_set_true(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.true_object());

        Ok(())
    }

    fn ins_set_false(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        thread.set_register(slot, self.false_object());

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
        let slot_index = try!(instruction.arg(0));
        let object     = instruction_object!(instruction, thread, 1);

        thread.set_register(slot_index, object);

        Ok(())
    }

    fn ins_set_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let name_index = try!(instruction.arg(2));
        let target     = instruction_object!(instruction, thread, 0);
        let source     = instruction_object!(instruction, thread, 1);
        let name       = try!(code.string(name_index));

        write_lock!(target).add_constant(name.clone(), source);

        Ok(())
    }

    fn ins_get_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let index      = try!(instruction.arg(0));
        let src        = instruction_object!(instruction, thread, 1);
        let name_index = try!(instruction.arg(2));
        let name       = try!(code.string(name_index));

        let object = try!(
            read_lock!(src).lookup_constant(name)
                .ok_or(format!("Undefined constant {}", name))
        );

        thread.set_register(index, object);

        Ok(())
    }

    fn ins_set_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let target_object = instruction_object!(instruction, thread, 0);
        let source_object = instruction_object!(instruction, thread, 1);
        let name_index    = try!(instruction.arg(2));
        let name          = try!(code.string(name_index));

        write_lock!(target_object)
            .add_attribute(name.clone(), source_object);

        Ok(())
    }

    fn ins_get_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let target_index = try!(instruction.arg(0));
        let source       = instruction_object!(instruction, thread, 1);
        let name_index   = try!(instruction.arg(2));
        let name         = try!(code.string(name_index));

        let attr = try!(
            read_lock!(source).lookup_attribute(name)
                .ok_or(format!("undefined attribute {}", name))
        );

        thread.set_register(target_index, attr);

        Ok(())
    }

    fn ins_send(&self, thread: RcThread, code: RcCompiledCode,
                instruction: &Instruction) -> EmptyResult {
        let result_slot   = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let name_index    = try!(instruction.arg(2));
        let allow_private = try!(instruction.arg(3));
        let arg_count     = try!(instruction.arg(4));
        let name          = try!(code.string(name_index));

        let receiver = read_lock!(receiver_lock);

        let method_code = try!(
            receiver.lookup_method(name)
                .ok_or(receiver.undefined_method_error(name))
        );

        if method_code.is_private() && allow_private == 0 {
            return Err(receiver.private_method_error(name));
        }

        let mut arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 5, arg_count)
        );

        if arguments.len() != method_code.required_arguments {
            return Err(format!(
                "{} requires {} arguments, {} given",
                name,
                method_code.required_arguments,
                arguments.len()
            ));
        }

        // Expose the receiver as "self" to the method
        arguments.insert(0, receiver_lock.clone());

        let retval = try!(
            self.run_code(thread.clone(), method_code, arguments)
        );

        if retval.is_some() {
            thread.set_register(result_slot, retval.unwrap());
        }

        Ok(())
    }

    fn ins_return(&self, thread: RcThread, _: RcCompiledCode,
                  instruction: &Instruction) -> OptionObjectResult {
        let slot = try!(instruction.arg(0));

        Ok(thread.get_register_option(slot))
    }

    fn ins_goto_if_false(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> OptionIntegerResult {
        let go_to      = try!(instruction.arg(0));
        let value_slot = try!(instruction.arg(1));
        let value      = thread.get_register_option(value_slot);

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
        let go_to      = try!(instruction.arg(0));
        let value_slot = try!(instruction.arg(1));
        let value      = thread.get_register_option(value_slot);

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

    fn ins_def_method(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let receiver_lock = instruction_object!(instruction, thread, 0);
        let name_index    = try!(instruction.arg(1));
        let code_index    = try!(instruction.arg(2));
        let name          = try!(code.string(name_index));
        let method_code   = try!(code.code_object(code_index)).clone();

        let mut receiver = write_lock!(receiver_lock);

        receiver.add_method(name.clone(), method_code);

        Ok(())
    }

    fn ins_run_code(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> EmptyResult {
        let result_index = try!(instruction.arg(0));
        let code_index   = try!(instruction.arg(1));
        let arg_count    = try!(instruction.arg(2));
        let code_obj     = try!(code.code_object(code_index)).clone();

        let arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 3, arg_count)
        );

        let retval = try!(self.run_code(thread.clone(), code_obj, arguments));

        if retval.is_some() {
            thread.set_register(result_index, retval.unwrap());
        }

        Ok(())
    }

    fn ins_get_toplevel(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot = try!(instruction.arg(0));

        let top_level = read_lock!(self.memory_manager).top_level.clone();

        thread.set_register(slot, top_level);

        Ok(())
    }

    fn ins_integer_add(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() + arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_div(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() / arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_mul(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() * arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_sub(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() - arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_mod(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() % arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_to_float(&self, thread: RcThread, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let slot         = try!(instruction.arg(0));
        let integer_lock = instruction_object!(instruction, thread, 1);
        let integer      = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer() as f64;
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_to_string(&self, thread: RcThread, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let slot         = try!(instruction.arg(0));
        let integer_lock = instruction_object!(instruction, thread, 1);

        let integer = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer().to_string();
        let obj    = self.allocate(object_value::string(result),
                                   self.string_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_and(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() & arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_or(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() | arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_xor(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() ^ arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_shift_left(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() << arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_shift_right(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_integers!(receiver, arg);

        let result = receiver.value.as_integer() >> arg.value.as_integer();
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_smaller(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_integer_greater(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_integer_equals(&self, thread: RcThread, _: RcCompiledCode,
                          instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_start_thread(&self, thread: RcThread, code: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot        = try!(instruction.arg(0));
        let code_index  = try!(instruction.arg(1));
        let thread_code = try!(code.code_object(code_index)).clone();

        let thread_object = self.run_thread(thread_code, false);

        thread.set_register(slot, thread_object);

        Ok(())
    }

    fn ins_float_add(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let added = receiver.value.as_float() + arg.value.as_float();
        let obj   = self.allocate(object_value::float(added),
                                  self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_mul(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() * arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_div(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() / arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_sub(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() - arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_mod(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
        let receiver_lock = instruction_object!(instruction, thread, 1);
        let arg_lock      = instruction_object!(instruction, thread, 2);

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() % arg.value.as_float();
        let obj    = self.allocate(object_value::float(result),
                                   self.float_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_to_integer(&self, thread: RcThread, _: RcCompiledCode,
                            instruction: &Instruction) -> EmptyResult {
        let slot       = try!(instruction.arg(0));
        let float_lock = instruction_object!(instruction, thread, 1);
        let float      = read_lock!(float_lock);

        ensure_floats!(float);

        let result = float.value.as_float() as isize;
        let obj    = self.allocate(object_value::integer(result),
                                   self.integer_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_to_string(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot       = try!(instruction.arg(0));
        let float_lock = instruction_object!(instruction, thread, 1);
        let float      = read_lock!(float_lock);

        ensure_floats!(float);

        let result = float.value.as_float().to_string();
        let obj    = self.allocate(object_value::string(result),
                                   self.string_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_smaller(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_float_greater(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_float_equals(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

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
        let slot       = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let index      = try!(instruction.arg(2));
        let array      = read_lock!(array_lock);

        ensure_arrays!(array);

        let vector = array.value.as_array();

        ensure_array_within_bounds!(vector, index);

        let value = vector[index].clone();

        thread.set_register(slot, value);

        Ok(())
    }

    fn ins_array_remove(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot       = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let index      = try!(instruction.arg(1));
        let mut array  = write_lock!(array_lock);

        ensure_arrays!(array);

        let mut vector = array.value.as_array_mut();

        ensure_array_within_bounds!(vector, index);

        let value = vector.remove(index);

        thread.set_register(slot, value);

        Ok(())
    }

    fn ins_array_length(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot       = try!(instruction.arg(0));
        let array_lock = instruction_object!(instruction, thread, 1);
        let array      = read_lock!(array_lock);

        ensure_arrays!(array);

        let vector = array.value.as_array();
        let length = vector.len() as isize;

        let obj = self.allocate(object_value::integer(length),
                                self.integer_prototype());

        thread.set_register(slot, obj);

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
        let slot        = try!(instruction.arg(0));
        let source_lock = instruction_object!(instruction, thread, 1);
        let source      = read_lock!(source_lock);

        ensure_strings!(source);

        let lower = source.value.as_string().to_lowercase();
        let obj   = self.allocate(object_value::string(lower),
                                  self.string_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_string_to_upper(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot        = try!(instruction.arg(0));
        let source_lock = instruction_object!(instruction, thread, 1);
        let source      = read_lock!(source_lock);

        ensure_strings!(source);

        let upper = source.value.as_string().to_uppercase();
        let obj   = self.allocate(object_value::string(upper),
                                  self.string_prototype());

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_string_equals(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let slot          = try!(instruction.arg(0));
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

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_string_to_bytes(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto   = self.integer_prototype();
        let array_proto = self.array_prototype();

        let array = arg.value.as_string().as_bytes().iter().map(|&b| {
            self.allocate(object_value::integer(b as isize), int_proto.clone())
        }).collect::<Vec<_>>();

        let obj = self.allocate(object_value::array(array), array_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_string_from_bytes(&self, thread: RcThread, _: RcCompiledCode,
                             instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
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

        let string = try!(map_error!(String::from_utf8(bytes)));
        let obj    = self.allocate(object_value::string(string), string_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_string_length(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto = self.integer_prototype();

        let length = arg.value.as_string().chars().count() as isize;
        let obj    = self.allocate(object_value::integer(length), int_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_string_size(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto = self.integer_prototype();

        let size = arg.value.as_string().len() as isize;
        let obj  = self.allocate(object_value::integer(size), int_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_stdout_write(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto  = self.integer_prototype();
        let mut stdout = io::stdout();

        let result = try!(
            map_error!(stdout.write(arg.value.as_string().as_bytes()))
        );

        let obj = self.allocate(object_value::integer(result as isize),
                                int_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_stderr_write(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> EmptyResult {
        let slot     = try!(instruction.arg(0));
        let arg_lock = instruction_object!(instruction, thread, 1);
        let arg      = read_lock!(arg_lock);

        ensure_strings!(arg);

        let int_proto  = self.integer_prototype();
        let mut stderr = io::stderr();

        let result = try!(
            map_error!(stderr.write(arg.value.as_string().as_bytes()))
        );

        let obj = self.allocate(object_value::integer(result as isize),
                                int_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_stdin_read(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let slot  = try!(instruction.arg(0));
        let proto = self.string_prototype();

        let mut buffer = file_reading_buffer!(instruction, thread, 1);

        try!(map_error!(io::stdin().read_to_string(&mut buffer)));

        let obj = self.allocate(object_value::string(buffer), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_stdin_read_line(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> EmptyResult {
        let slot  = try!(instruction.arg(0));
        let proto = self.string_prototype();

        let mut buffer = String::new();

        try!(map_error!(io::stdin().read_line(&mut buffer)));

        let obj = self.allocate(object_value::string(buffer), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_file_open(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot      = try!(instruction.arg(0));
        let path_lock = instruction_object!(instruction, thread, 1);
        let mode_lock = instruction_object!(instruction, thread, 2);

        let file_proto = self.file_prototype();

        let path = read_lock!(path_lock);
        let mode = read_lock!(mode_lock);

        let path_string = path.value.as_string();
        let mode_string = mode.value.as_string().as_ref();

        let open_opts = try!(open_options::from_fopen_string(mode_string));
        let file      = try!(map_error!(open_opts.open(path_string)));

        let obj = self.allocate(object_value::file(file), file_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_file_write(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> EmptyResult {
        let slot        = try!(instruction.arg(0));
        let file_lock   = instruction_object!(instruction, thread, 1);
        let string_lock = instruction_object!(instruction, thread, 2);

        let mut file = write_lock!(file_lock);
        let string   = read_lock!(string_lock);

        ensure_files!(file);
        ensure_strings!(string);

        let int_proto = self.integer_prototype();
        let mut file  = file.value.as_file_mut();
        let bytes     = string.value.as_string().as_bytes();

        let result = try!(map_error!(file.write(bytes)));

        let obj = self.allocate(object_value::integer(result as isize),
                                int_proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_file_read(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> EmptyResult {
        let slot         = try!(instruction.arg(0));
        let file_lock    = instruction_object!(instruction, thread, 1);
        let mut file_obj = write_lock!(file_lock);

        ensure_files!(file_obj);

        let mut buffer = file_reading_buffer!(instruction, thread, 2);
        let int_proto  = self.integer_prototype();
        let mut file   = file_obj.value.as_file_mut();

        try!(map_error!(file.read_to_string(&mut buffer)));

        let obj = self.allocate(object_value::string(buffer), int_proto);

        thread.set_register(slot, obj);

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

    fn run_code(&self, thread: RcThread, code: RcCompiledCode,
                args: Vec<RcObject>) -> OptionObjectResult {
        // Scoped so the the RwLock is local to the block, allowing recursive
        // calling of the "run" method.
        {
            thread.push_call_frame(CallFrame::from_code(code.clone()));

            for arg in args.iter() {
                thread.add_local(arg.clone());
            }
        }

        let return_val = try!(self.run(thread.clone(), code));

        thread.pop_call_frame();

        Ok(return_val)
    }

    fn collect_arguments(&self, thread: RcThread, instruction: &Instruction,
                         offset: usize, amount: usize) -> ObjectVecResult {
        let mut args: Vec<RcObject> = Vec::new();

        for index in offset..(offset + amount) {
            let arg_index = instruction.arguments[index];
            let arg       = try!(thread.get_register(arg_index));

            args.push(arg)
        }

        Ok(args)
    }

    fn run_thread(&self, code: RcCompiledCode, main_thread: bool) -> RcObject {
        let self_clone = self.clone();
        let code_clone = code.clone();

        let (chan_sender, chan_receiver) = channel();

        let handle = thread::spawn(move || {
            let thread_obj: RcObject = chan_receiver.recv().unwrap();
            let vm_thread = read_lock!(thread_obj).value.as_thread();

            let result = self_clone.run(vm_thread.clone(), code_clone);

            write_lock!(self_clone.threads).remove(thread_obj.clone());

            // After this there's a chance thread_obj might be GC'd so we can't
            // reliably use it any more.
            write_lock!(thread_obj).unpin();

            match result {
                Ok(obj) => {
                    vm_thread.set_value(obj);
                },
                Err(message) => {
                    self_clone.error(vm_thread, message);

                    write_lock!(self_clone.threads).stop();
                }
            };
        });

        let vm_thread = Thread::from_code(code.clone(), Some(handle));

        let thread_obj = write_lock!(self.memory_manager)
            .allocate_thread(vm_thread.clone());

        write_lock!(self.threads).add(thread_obj.clone());

        if main_thread {
            vm_thread.set_main();
        }

        chan_sender.send(thread_obj.clone()).unwrap();

        thread_obj
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
    use thread::Thread;

    macro_rules! compiled_code {
        ($ins: expr) => (
            CompiledCode::new("test".to_string(), "test".to_string(), 1, $ins)
        );
    }

    macro_rules! call_frame {
        () => (
            CallFrame::new("foo".to_string(), "foo".to_string(), 1)
        );
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

        cc.add_integer_literal(10);

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        let int_obj = thread.get_register(1).unwrap();
        let value   = read_lock!(int_obj).value.as_integer();

        assert!(result.is_ok());

        assert_eq!(value, 10);
    }
}
