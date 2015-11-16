//! Virtual Machine for running instructions
//!
//! A VirtualMachine manages threads, runs instructions, starts/terminates
//! threads and so on. VirtualMachine instances are fully self contained
//! allowing multiple instances to run fully isolated in the same process.

use std::io::{self, Write};
use std::thread;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use instruction::{InstructionType, Instruction};
use memory_manager::{MemoryManager, RcMemoryManager};
use object::RcObject;
use object_value;
use virtual_machine_methods::VirtualMachineMethods;
use thread::{Thread, RcThread};
use thread_list::ThreadList;

/// Calls an instruction method on a given receiver.
macro_rules! run {
    ($rec: expr, $name: ident, $thread: ident, $code: ident, $ins: ident) => (
        try!($rec.$name($thread.clone(), $code.clone(), &$ins));
    );
}

macro_rules! error_when_prototype_exists {
    ($rec: expr, $name: ident) => (
        if read_lock!($rec.memory_manager).$name().is_some() {
            return Err("prototype already defined".to_string());
        }
    );
}

macro_rules! ensure_integers {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_integer() {
                return Err("all objects must be integers".to_string());
            }
        )+
    );
}

macro_rules! ensure_floats {
    ($($ident: ident),+) => (
        $(
            if !$ident.value.is_float() {
                return Err("all objects must be floats".to_string());
            }
        )+
    );
}

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

    fn integer_prototype(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .integer_prototype()
            .ok_or("no integer prototype set up".to_string())
    }

    fn float_prototype(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .float_prototype()
            .ok_or("no float prototype set up".to_string())
    }

    fn string_prototype(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .string_prototype()
            .ok_or("no string prototype set up".to_string())
    }

    fn array_prototype(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .array_prototype()
            .ok_or("no array prototype set up".to_string())
    }

    fn thread_prototype(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .thread_prototype()
            .ok_or("no thread prototype set up".to_string())
    }

    fn false_object(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .false_object()
            .ok_or("no false object set up".to_string())
    }

    fn true_object(&self) -> Result<RcObject, String> {
        read_lock!(self.memory_manager)
            .true_object()
            .ok_or("no true object set up".to_string())
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

    fn run(&self, thread: RcThread,
               code: RcCompiledCode) -> Result<Option<RcObject>, String> {
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
                InstructionType::SetIntegerPrototype => {
                    run!(self, ins_set_integer_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetFloatPrototype => {
                    run!(self, ins_set_float_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetStringPrototype => {
                    run!(self, ins_set_string_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetArrayPrototype => {
                    run!(self, ins_set_array_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetThreadPrototype => {
                    run!(self, ins_set_thread_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetTruePrototype => {
                    run!(self, ins_set_true_prototype, thread, code,
                         instruction);
                },
                InstructionType::SetFalsePrototype => {
                    run!(self, ins_set_false_prototype, thread, code,
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
                InstructionType::IntegerEqual => {
                    run!(self, ins_integer_equal, thread, code, instruction);
                },
                InstructionType::StartThread => {
                    run!(self, ins_start_thread, thread, code, instruction);
                },
                InstructionType::FloatAdd => {
                    run!(self, ins_float_add, thread, code, instruction);
                },
                InstructionType::FloatMul => {
                    run!(self, ins_float_mul, thread, code, instruction);
                }
            };
        }

        Ok(retval)
    }

    fn ins_set_integer(&self, thread: RcThread, code: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot  = *try!(instruction.arg(0));
        let index = *try!(instruction.arg(1));
        let value = *try!(code.integer(index));

        let proto = try!(self.integer_prototype());
        let obj   = self.allocate(object_value::integer(value), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_float(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot  = *try!(instruction.arg(0));
        let index = *try!(instruction.arg(1));
        let value = *try!(code.float(index));

        let proto = try!(self.float_prototype());
        let obj   = self.allocate(object_value::float(value), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_string(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let slot  = *try!(instruction.arg(0));
        let index = *try!(instruction.arg(1));
        let value = try!(code.string(index));

        let proto = try!(self.string_prototype());
        let obj   = self.allocate(object_value::string(value.clone()), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_object(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(instruction.arg(0));

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
                     instruction: &Instruction) -> Result<(), String> {
        let slot      = *try!(instruction.arg(0));
        let val_count = *try!(instruction.arg(1));

        let values = try!(
            self.collect_arguments(thread.clone(), instruction, 2, val_count)
        );

        let proto = try!(self.array_prototype());
        let obj   = self.allocate(object_value::array(values), proto);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_set_name(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let slot       = *try!(instruction.arg(0));
        let name_index = *try!(instruction.arg(1));

        let obj  = try!(thread.get_register(slot));
        let name = try!(code.string(name_index));

        write_lock!(obj).set_name(name.clone());

        Ok(())
    }

    fn ins_set_integer_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                 instruction: &Instruction) -> Result<(), String> {
        error_when_prototype_exists!(self, integer_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_integer_prototype(object);

        Ok(())
    }

    fn ins_set_float_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        error_when_prototype_exists!(self, float_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_float_prototype(object);

        Ok(())
    }

    fn ins_set_string_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> Result<(), String> {
        error_when_prototype_exists!(self, string_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_string_prototype(object);

        Ok(())
    }

    fn ins_set_array_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction)
                               -> Result<(), String> {
        error_when_prototype_exists!(self, array_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_array_prototype(object);

        Ok(())
    }

    fn ins_set_thread_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction)
                                -> Result<(), String> {
        error_when_prototype_exists!(self, thread_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_thread_prototype(object.clone());

        // Update the prototype of all existing threads (usually only the main
        // thread at this point).
        write_lock!(self.threads).set_prototype(object);

        Ok(())
    }

    fn ins_set_true_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> Result<(), String> {
        error_when_prototype_exists!(self, true_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_true_prototype(object.clone());

        Ok(())
    }

    fn ins_set_false_prototype(&self, thread: RcThread, _: RcCompiledCode,
                              instruction: &Instruction) -> Result<(), String> {
        error_when_prototype_exists!(self, false_prototype);

        let slot   = *try!(instruction.arg(0));
        let object = try!(thread.get_register(slot));

        write_lock!(self.memory_manager).set_false_prototype(object.clone());

        Ok(())
    }

    fn ins_set_true(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let slot   = *try!(instruction.arg(0));
        let object = try!(self.true_object());

        thread.set_register(slot, object);

        Ok(())
    }

    fn ins_set_false(&self, thread: RcThread, _: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let slot   = *try!(instruction.arg(0));
        let object = try!(self.false_object());

        thread.set_register(slot, object);

        Ok(())
    }

    fn ins_set_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let local_index  = *try!(instruction.arg(0));
        let object_index = *try!(instruction.arg(1));

        let object = try!(thread.get_register(object_index));

        thread.set_local(local_index, object);

        Ok(())
    }

    fn ins_get_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot_index  = *try!(instruction.arg(0));
        let local_index = *try!(instruction.arg(1));

        let object = try!(thread.get_local(local_index));

        thread.set_register(slot_index, object);

        Ok(())
    }

    fn ins_set_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let target_slot = *try!(instruction.arg(0));
        let source_slot = *try!(instruction.arg(1));
        let name_index  = *try!(instruction.arg(2));

        let target = try!(thread.get_register(target_slot));
        let source = try!(thread.get_register(source_slot));
        let name   = try!(code.string(name_index));

        write_lock!(target).add_constant(name.clone(), source);

        Ok(())
    }

    fn ins_get_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let index      = *try!(instruction.arg(0));
        let src_index  = *try!(instruction.arg(1));
        let name_index = *try!(instruction.arg(2));

        let name = try!(code.string(name_index));
        let src  = try!(thread.get_register(src_index));

        let object = try!(
            read_lock!(src).lookup_constant(name)
                .ok_or(format!("Undefined constant {}", name))
        );

        thread.set_register(index, object);

        Ok(())
    }

    fn ins_set_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let target_index  = *try!(instruction.arg(0));
        let source_index  = *try!(instruction.arg(1));
        let name_index    = *try!(instruction.arg(2));
        let target_object = try!(thread.get_register(target_index));
        let source_object = try!(thread.get_register(source_index));
        let name          = try!(code.string(name_index));

        write_lock!(target_object)
            .add_attribute(name.clone(), source_object);

        Ok(())
    }

    fn ins_get_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let target_index = *try!(instruction.arg(0));
        let source_index = *try!(instruction.arg(1));
        let name_index   = *try!(instruction.arg(2));
        let source       = try!(thread.get_register(source_index));
        let name         = try!(code.string(name_index));

        let attr = try!(
            read_lock!(source).lookup_attribute(name)
                .ok_or(format!("undefined attribute {}", name))
        );

        thread.set_register(target_index, attr);

        Ok(())
    }

    fn ins_send(&self, thread: RcThread, code: RcCompiledCode,
                instruction: &Instruction) -> Result<(), String> {
        let result_slot   = *try!(instruction.arg(0));
        let receiver_slot = *try!(instruction.arg(1));
        let name_index    = *try!(instruction.arg(2));
        let allow_private = *try!(instruction.arg(3));
        let arg_count     = *try!(instruction.arg(4));
        let name          = try!(code.string(name_index));
        let receiver_lock = try!(thread.get_register(receiver_slot));

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
                  instruction: &Instruction)
                  -> Result<Option<RcObject>, String> {
        let slot = *try!(instruction.arg(0));

        Ok(thread.get_register_option(slot))
    }

    fn ins_goto_if_false(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction)
                         -> Result<Option<usize>, String> {
        let go_to      = *try!(instruction.arg(0));
        let value_slot = *try!(instruction.arg(1));
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
                       instruction: &Instruction)
                       -> Result<Option<usize>, String> {
        let go_to      = *try!(instruction.arg(0));
        let value_slot = *try!(instruction.arg(1));
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
                instruction: &Instruction) -> Result<usize, String> {
        let go_to = *try!(instruction.arg(0));

        Ok(go_to)
    }

    fn ins_def_method(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let receiver_index = *try!(instruction.arg(0));
        let name_index     = *try!(instruction.arg(1));
        let code_index     = *try!(instruction.arg(2));
        let receiver_lock  = try!(thread.get_register(receiver_index));
        let name           = try!(code.string(name_index));
        let method_code    = try!(code.code_object(code_index)).clone();

        let mut receiver = write_lock!(receiver_lock);

        receiver.add_method(name.clone(), method_code);

        Ok(())
    }

    fn ins_run_code(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let result_index = *try!(instruction.arg(0));
        let code_index   = *try!(instruction.arg(1));
        let arg_count    = *try!(instruction.arg(2));
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
                        instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(instruction.arg(0));

        let top_level = read_lock!(self.memory_manager).top_level.clone();

        thread.set_register(slot, top_level);

        Ok(())
    }

    fn ins_integer_add(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let added = left_object.value.as_integer() +
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(added), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_div(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() /
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_mul(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() *
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_sub(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() -
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_mod(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() %
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_to_float(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot         = *try!(instruction.arg(0));
        let int_index    = *try!(instruction.arg(1));
        let integer_lock = try!(thread.get_register(int_index));
        let prototype    = try!(self.float_prototype());
        let integer      = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer() as f64;

        let obj = self.allocate(object_value::float(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_to_string(&self, thread: RcThread, _: RcCompiledCode,
                             instruction: &Instruction) -> Result<(), String> {
        let slot         = *try!(instruction.arg(0));
        let int_index    = *try!(instruction.arg(1));
        let integer_lock = try!(thread.get_register(int_index));
        let prototype    = try!(self.string_prototype());

        let integer = read_lock!(integer_lock);

        ensure_integers!(integer);

        let result = integer.value.as_integer().to_string();

        let obj = self.allocate(object_value::string(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_and(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!( thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() &
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_or(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() |
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_bitwise_xor(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() ^
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_shift_left(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() <<
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_shift_right(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));
        let prototype         = try!(self.integer_prototype());

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let result = left_object.value.as_integer() >>
            right_object.value.as_integer();

        let obj = self.allocate(object_value::integer(result), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_integer_smaller(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let smaller = left_object.value.as_integer() <
            right_object.value.as_integer();

        let boolean = if smaller {
            try!(self.true_object())
        }
        else {
            try!(self.false_object())
        };

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_integer_greater(&self, thread: RcThread, _: RcCompiledCode,
                           instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let smaller = left_object.value.as_integer() >
            right_object.value.as_integer();

        let boolean = if smaller {
            try!(self.true_object())
        }
        else {
            try!(self.false_object())
        };

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_integer_equal(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> Result<(), String> {
        let slot              = *try!(instruction.arg(0));
        let left_index        = *try!(instruction.arg(1));
        let right_index       = *try!(instruction.arg(2));
        let left_object_lock  = try!(thread.get_register(left_index));
        let right_object_lock = try!(thread.get_register(right_index));

        let left_object  = read_lock!(left_object_lock);
        let right_object = read_lock!(right_object_lock);

        ensure_integers!(left_object, right_object);

        let smaller = left_object.value.as_integer() ==
            right_object.value.as_integer();

        let boolean = if smaller {
            try!(self.true_object())
        }
        else {
            try!(self.false_object())
        };

        thread.set_register(slot, boolean);

        Ok(())
    }

    fn ins_start_thread(&self, thread: RcThread, code: RcCompiledCode,
                        instruction: &Instruction) -> Result<(), String> {
        let slot        = *try!(instruction.arg(0));
        let code_index  = *try!(instruction.arg(1));
        let thread_code = try!(code.code_object(code_index)).clone();

        try!(self.thread_prototype());

        let thread_object = self.run_thread(thread_code, false);

        thread.set_register(slot, thread_object);

        Ok(())
    }

    fn ins_float_add(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot           = *try!(instruction.arg(0));
        let receiver_index = *try!(instruction.arg(1));
        let arg_index      = *try!(instruction.arg(2));
        let receiver_lock  = try!(thread.get_register(receiver_index));
        let arg_lock       = try!(thread.get_register(arg_index));
        let prototype      = try!(self.float_prototype());

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let added = receiver.value.as_float() + arg.value.as_float();
        let obj   = self.allocate(object_value::float(added), prototype);

        thread.set_register(slot, obj);

        Ok(())
    }

    fn ins_float_mul(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot           = *try!(instruction.arg(0));
        let receiver_index = *try!(instruction.arg(1));
        let arg_index      = *try!(instruction.arg(2));
        let receiver_lock  = try!(thread.get_register(receiver_index));
        let arg_lock       = try!(thread.get_register(arg_index));
        let prototype      = try!(self.float_prototype());

        let receiver = read_lock!(receiver_lock);
        let arg      = read_lock!(arg_lock);

        ensure_floats!(receiver, arg);

        let result = receiver.value.as_float() * arg.value.as_float();
        let obj    = self.allocate(object_value::float(result), prototype);

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
                args: Vec<RcObject>) -> Result<Option<RcObject>, String> {
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
                         offset: usize,
                         amount: usize) -> Result<Vec<RcObject>, String> {
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
    fn test_ins_set_integer_without_integer_prototype() {
        let vm = VirtualMachine::new();

        let mut cc = compiled_code!(
            vec![instruction!(InstructionType::SetInteger, vec![0, 0])]
        );

        cc.add_integer_literal(10);

        let thread = Thread::new(call_frame!(), None);
        let result = run!(vm, thread, cc);

        assert!(result.is_err());
    }

    #[test]
    fn test_ins_set_integer_with_valid_arguments() {
        let vm = VirtualMachine::new();

        let mut cc = compiled_code!(
            vec![
                instruction!(InstructionType::SetObject, vec![0]),
                instruction!(InstructionType::SetIntegerPrototype, vec![0]),
                instruction!(InstructionType::SetInteger, vec![1, 0])
            ]
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
