use std::io::{self, Write};
use std::thread;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use instruction::{InstructionType, Instruction};
use memory_manager::MemoryManager;
use object::{Object, ObjectValue, RcObject};
use thread::{Thread, RcThread};

/// A reference counted VirtualMachine.
pub type RcVirtualMachine = Arc<VirtualMachine>;

/// Structure representing a single VM instance.
///
/// A VirtualMachine manages threads, runs instructions, starts/terminates
/// threads and so on. VirtualMachine instances are fully self contained
/// allowing multiple instances to run fully isolated in the same process.
///
pub struct VirtualMachine {
    // All threads that are currently active.
    threads: RwLock<Vec<RcObject>>,

    // The struct for allocating/managing memory.
    memory_manager: MemoryManager
}

impl VirtualMachine {
    /// Creates a new VirtualMachine.
    pub fn new() -> RcVirtualMachine {
        let vm = VirtualMachine {
            threads: RwLock::new(Vec::new()),
            memory_manager: MemoryManager::new()
        };

        Arc::new(vm)
    }
}

pub trait ArcMethods {
    /// Starts the main thread
    ///
    /// This requires a CompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    ///
    fn start(&self, RcCompiledCode) -> Result<(), ()>;

    /// Runs a CompiledCode for a specific Thread.
    ///
    /// This iterates over all instructions in the CompiledCode, executing them
    /// one by one (except when certain instructions dictate otherwise).
    ///
    /// The return value is whatever the last CompiledCode returned (if
    /// anything). Values are only returned when a CompiledCode ends with a
    /// "return" instruction.
    ///
    fn run(&self, RcThread, RcCompiledCode)
        -> Result<Option<RcObject>, String>;

    /// Allocates and sets an integer in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the integer in.
    /// 2. The index of the integer literals to use for the value.
    ///
    /// The integer literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///
    fn ins_set_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Allocates and sets a float in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the float in.
    /// 2. The index of the float literals to use for the value.
    ///
    /// The float literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     float_literals
    ///       0: 10.5
    ///
    ///     0: set_float 0, 0
    ///
    fn ins_set_float(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Allocates and sets a string in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index to store the float in.
    /// 2. The index of the string literal to use for the value.
    ///
    /// The string literal is extracted from the given CompiledCode.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     set_string 0, 0
    ///
    fn ins_set_string(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Allocates and sets an object in a register slot.
    ///
    /// This instruction requires at least one argument: the slot to store the
    /// object in. Optionally an extra argument can be provided, this argument
    /// should be a slot index pointing to the object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_object 1, 0
    ///
    fn ins_set_object(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Allocates and sets an array in a register slot.
    ///
    /// This instruction requires at least two arguments:
    ///
    /// 1. The register slot to store the array in.
    /// 2. The amount of values to store in the array.
    ///
    /// If the 2nd argument is N where N > 0 then all N following arguments are
    /// used as values for the array.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_array_prototype 0
    ///     2: set_object          1
    ///     3: set_object          2
    ///     4: set_array           3, 2, 1, 2
    ///
    fn ins_set_array(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the name of a given object.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The slot index containing the object to name.
    /// 2. The string literal index containing the name of the object.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Foo"
    ///
    ///     0: set_object 0
    ///     1: set_name   0, 0
    ///
    fn ins_set_name(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Integer objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object            0
    ///     1: set_integer_prototype 0
    ///
    fn ins_set_integer_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Float objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_float_prototype 0
    ///
    fn ins_set_float_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for String objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object           0
    ///     1: set_string_prototype 0
    ///
    fn ins_set_string_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Array objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object          0
    ///     1: set_array_prototype 0
    ///
    fn ins_set_array_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the prototype for Thread objects.
    ///
    /// This instruction requires one argument: the slot index pointing to an
    /// object to use as the prototype.
    ///
    /// This instruction also updates any existing threads with the new
    /// prototype.
    ///
    /// # Examples
    ///
    ///     0: set_object           0
    ///     1: set_thread_prototype 0
    ///
    fn ins_set_thread_prototype(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a local variable to a given slot's value.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The local variable index to set.
    /// 2. The slot index containing the object to store in the variable.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    ///
    fn ins_set_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Gets a local variable and stores it in a register slot.
    ///
    /// This instruction requires two arguments:
    ///
    /// 1. The register slot to store the local's value in.
    /// 2. The local variable index to get the value from.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///     1: set_local  0, 0
    ///     2: get_local  1, 0
    ///
    fn ins_get_local(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets a constant in a given object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The slot index pointing to the object to store the constant in.
    /// 2. The slot index pointing to the object to store.
    /// 3. The string literal index to use for the constant name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Object"
    ///
    ///     0: get_toplevel 0
    ///     1: set_object   1
    ///     2: set_name     1, 0
    ///     3: set_const    0, 1, 0
    ///
    fn ins_set_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Looks up a constant and stores it in a register slot.
    ///
    /// This instruction takes 3 arguments:
    ///
    /// 1. The slot index to store the constant in.
    /// 2. The slot index pointing to an object in which to look for the
    ///    constant.
    /// 3. The string literal index containing the name of the constant.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "Object"
    ///
    ///     0: get_const 0, 0
    ///
    fn ins_get_const(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets an attribute value in a specific object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot containing the object for which to set the
    ///    attribute.
    /// 2. The register slot containing the object to set as the attribute
    ///    value.
    /// 3. A string literal index pointing to the name to use for the attribute.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_attr   1, 0, 0
    ///
    fn ins_set_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Gets an attribute from an object and stores it in a register slot.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the attribute's value in.
    /// 2. The register slot containing the object from which to retrieve the
    ///    attribute.
    /// 3. A string literal index pointing to the attribute name.
    ///
    /// # Examples
    ///
    ///     string_literals
    ///       0: "foo"
    ///
    ///     0: set_object 0
    ///     1: set_object 1
    ///     2: set_attr   1, 0, 0
    ///     3: get_attr   2, 1, 0
    ///
    fn ins_get_attr(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sends a message and stores the result in a register slot.
    ///
    /// This instruction requires at least 4 arguments:
    ///
    /// 1. The slot index to store the result in.
    /// 2. The slot index of the receiver.
    /// 3. The index of the string literals to use for the method name.
    /// 4. A boolean (1 or 0) indicating if private methods can be called.
    /// 5. The amount of arguments to pass (0 or more).
    ///
    /// If the argument amount is set to N where N > 0 then the N instruction
    /// arguments following the 5th instruction argument are used as arguments
    /// for sending the message.
    ///
    /// This instruction does not allocate a String for the method name.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     string_literals
    ///       0: "+"
    ///
    ///     0: set_integer 0, 0              # 10
    ///     1: set_integer 1, 1              # 20
    ///     2: send        2, 0, 0, 0, 1, 1  # 10.+(20)
    ///
    fn ins_send(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Returns the value in the given register slot.
    ///
    /// As register slots can be left empty this method returns an Option
    /// instead of returning an Object directly.
    ///
    /// This instruction takes a single argument: the slot index containing the
    /// value to return.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///     1: return      0
    ///
    fn ins_return(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<RcObject>, String>;

    /// Jumps to an instruction if a slot is not set.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a slot is not set.
    /// 2. The slot index to check.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     0: goto_if_undef 0, 1
    ///     1: set_integer   0, 0
    ///     2: set_integer   0, 1
    ///
    /// Here slot "0" would be set to "20".
    ///
    fn ins_goto_if_undef(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<usize>, String>;

    /// Jumps to an instruction if a slot is set.
    ///
    /// This instruction takes two arguments:
    ///
    /// 1. The instruction index to jump to if a slot is set.
    /// 2. The slot index to check.
    ///
    /// # Examples
    ///
    ///     integer_literals
    ///       0: 10
    ///       1: 20
    ///
    ///     0: set_integer   0, 0
    ///     1: goto_if_def   3, 0
    ///     2: set_integer   0, 1
    ///
    /// Here slot "0" would be set to "10".
    ///
    fn ins_goto_if_def(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<Option<usize>, String>;

    /// Defines a method for an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. A slot index pointing to a specific object to define the method on.
    /// 2. The string literal index containing the method name.
    /// 3. The code object index containing the CompiledCode of the method.
    ///
    fn ins_def_method(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Runs a CompiledCode.
    ///
    /// This instruction takes at least 3 arguments:
    ///
    /// 1. The slot index to store the return value in.
    /// 2. The code object index pointing to the CompiledCode to run.
    /// 3. The amount of arguments to pass to the CompiledCode.
    ///
    /// If the amount of arguments is greater than 0 any following arguments are
    /// used as slot indexes for retrieving the arguments to pass to the
    /// CompiledCode.
    ///
    fn ins_run_code(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Sets the top-level object in a register slot.
    ///
    /// This instruction requires one argument: the slot to store the object in.
    ///
    /// # Examples
    ///
    ///     get_toplevel 0
    ///
    fn ins_get_toplevel(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Performs an integer addition
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. The register slot to store the result in.
    /// 2. The register slot of the left-hand side object.
    /// 3. The register slot of the right-hand side object.
    ///
    /// # Examples
    ///
    ///     integer_literals:
    ///       0: 10
    ///       1: 20
    ///
    ///     0: set_integer 0, 0
    ///     1: set_integer 1, 1
    ///     2: add_integer 2, 0, 1
    ///
    fn ins_add_integer(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Runs a CompiledCode in a new thread.
    ///
    /// This instruction requires 2 arguments:
    ///
    /// 1. The register slot to store the thread object in.
    /// 2. A code objects index pointing to the CompiledCode object to run.
    ///
    /// # Examples
    ///
    ///     code_objects
    ///       0: CompiledCode(name="foo")
    ///
    ///     0: set_object           0
    ///     1: set_thread_prototype 0
    ///     2. start_thread         1, 0
    ///
    fn ins_start_thread(&self, RcThread, RcCompiledCode, &Instruction)
        -> Result<(), String>;

    /// Prints a VM backtrace of a given thread with a message.
    fn print_error(&self, RcThread, String);

    /// Runs a given CompiledCode with arguments.
    fn run_code(&self, RcThread, RcCompiledCode, Vec<RcObject>)
        -> Result<Option<RcObject>, String>;

    /// Collects a set of arguments from an instruction.
    fn collect_arguments(&self, RcThread, &Instruction, usize, usize)
        -> Result<Vec<RcObject>, String>;

    /// Runs a CompiledCode in a new thread.
    fn run_thread(&self, RcCompiledCode) -> RcObject;

    /// Allocates a new Thread Object
    fn allocate_thread(&self, RcThread) -> RcObject;
}

impl ArcMethods for RcVirtualMachine {
    fn start(&self, code: RcCompiledCode) -> Result<(), ()> {
        let vm_thread = Thread::from_code(code.clone(), None);

        vm_thread.write().unwrap().set_main();

        self.allocate_thread(vm_thread.clone());

        let result = self.run(vm_thread.clone(), code);

        // TODO: replace this with a supervisor thread
        match result {
            Ok(_)        => Ok(()),
            Err(message) => {
                self.print_error(vm_thread, message);

                Err(())
            }
        }
    }

    fn run(&self, thread: RcThread,
               code: RcCompiledCode) -> Result<Option<RcObject>, String> {
        let mut skip_until: Option<usize> = None;
        let mut retval = None;

        for (index, instruction) in code.instructions.iter().enumerate() {
            if skip_until.is_some() {
                if index < skip_until.unwrap() {
                    continue;
                }
                else {
                    skip_until = None;
                }
            }

            match instruction.instruction_type {
                InstructionType::SetInteger => {
                    try!(self.ins_set_integer(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetFloat => {
                    try!(self.ins_set_float(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetString => {
                    try!(self.ins_set_string(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetObject => {
                    try!(self.ins_set_object(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetArray => {
                    try!(self.ins_set_array(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetName => {
                    try!(self.ins_set_name(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetIntegerPrototype => {
                    try!(self.ins_set_integer_prototype(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetFloatPrototype => {
                    try!(self.ins_set_float_prototype(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetStringPrototype => {
                    try!(self.ins_set_string_prototype(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetArrayPrototype => {
                    try!(self.ins_set_array_prototype(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetThreadPrototype => {
                    try!(self.ins_set_thread_prototype(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetLocal => {
                    try!(self.ins_set_local(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GetLocal => {
                    try!(self.ins_get_local(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetConst => {
                    try!(self.ins_set_const(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GetConst => {
                    try!(self.ins_get_const(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::SetAttr => {
                    try!(self.ins_set_attr(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GetAttr => {
                    try!(self.ins_get_attr(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::Send => {
                    try!(self.ins_send(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::Return => {
                    retval = try!(self.ins_return(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GotoIfUndef => {
                    skip_until = try!(self.ins_goto_if_undef(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GotoIfDef => {
                    skip_until = try!(self.ins_goto_if_def(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::DefMethod => {
                    try!(self.ins_def_method(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::RunCode => {
                    try!(self.ins_run_code(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::GetToplevel => {
                    try!(self.ins_get_toplevel(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::AddInteger => {
                    try!(self.ins_add_integer(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                },
                InstructionType::StartThread => {
                    try!(self.ins_start_thread(
                        thread.clone(),
                        code.clone(),
                        &instruction
                    ));
                }
            };
        }

        Ok(retval)
    }

    fn ins_set_integer(&self, thread: RcThread, code: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_integer: missing target slot".to_string())
        );

        let index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_integer: missing integer literal index".to_string())
        );

        let value = *try!(
            code.integer_literals
                .get(index)
                .ok_or("set_integer: undefined integer literal".to_string())
        );

        let prototype = try!(
            self.memory_manager
                .integer_prototype()
                .ok_or("set_integer: no Integer prototype set up".to_string())
        );

        let obj = self.memory_manager
            .allocate(ObjectValue::Integer(value), prototype.clone());

        thread.write().unwrap().register().set(slot, obj);

        Ok(())
    }

    fn ins_set_float(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_float: missing target slot".to_string())
        );

        let index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_float: missing float literal index".to_string())
        );

        let value  = *try!(
            code.float_literals
                .get(index)
                .ok_or("set_float: undefined float literal".to_string())
        );

        let prototype = try!(
            self.memory_manager
                .float_prototype()
                .ok_or("set_float: no Float prototype set up".to_string())
        );

        let obj = self.memory_manager
            .allocate(ObjectValue::Float(value), prototype.clone());

        thread.write().unwrap().register().set(slot, obj);

        Ok(())
    }

    fn ins_set_string(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_string: missing slot index".to_string())
        );

        let index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_string: missing string literal index".to_string())
        );

        let value = try!(
            code.string_literals
                .get(index)
                .ok_or("set_string: undefined string literal".to_string())
        );

        let prototype = try!(
            self.memory_manager
                .string_prototype()
                .ok_or("set_string: no String prototype set up".to_string())
        );

        let obj = self.memory_manager
            .allocate(ObjectValue::String(value.clone()), prototype.clone());

        thread.write().unwrap().register().set(slot, obj);

        Ok(())
    }

    fn ins_set_object(&self, thread: RcThread, _: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_object: missing slot index".to_string())
        );

        let proto_index_opt = instruction.arguments.get(1);

        let obj = Object::new(ObjectValue::None);

        if proto_index_opt.is_some() {
            let proto_index = *proto_index_opt.unwrap();

            let proto = try!(
                thread_ref.register()
                    .get(proto_index)
                    .ok_or("set_object: prototype is undefined".to_string())
            );

            obj.write().unwrap().set_prototype(proto);
        }

        self.memory_manager.allocate_prepared(obj.clone());

        thread_ref.register().set(slot, obj);

        Ok(())
    }

    fn ins_set_array(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_array: missing slot index".to_string())
        );

        let val_count = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_array: missing value count".to_string())
        );

        let values = try!(
            self.collect_arguments(thread.clone(), instruction, 2, val_count)
        );

        let prototype = try!(
            self.memory_manager
                .array_prototype()
                .ok_or("set_array: no Array prototype set up".to_string())
        );

        let obj = self.memory_manager
            .allocate(ObjectValue::Array(values), prototype.clone());

        thread.write().unwrap().register().set(slot, obj);

        Ok(())
    }

    fn ins_set_name(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_name: missing object slot".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_name: missing string literal index".to_string())
        );

        let obj = try!(
            thread_ref.register()
                .get(slot)
                .ok_or("set_name: undefined target object".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("set_name: undefined string literal".to_string())
        );

        obj.write().unwrap().set_name(name.clone());

        Ok(())
    }

    fn ins_set_integer_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                 instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_integer_prototype: missing object slot")
        );

        let object = try!(
            thread_ref.register()
                .get(slot)
                .ok_or("set_integer_prototype: undefined source object")
        );

        self.memory_manager.set_integer_prototype(object);

        Ok(())
    }

    fn ins_set_float_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_float_prototype: missing object slot")
        );

        let object = try!(
            thread_ref.register()
                .get(slot)
                .ok_or("set_float_prototype: undefined source object")
        );

        self.memory_manager.set_float_prototype(object);

        Ok(())
    }

    fn ins_set_string_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_string_prototype: missing object slot")
        );

        let object = try!(
            thread_ref.register()
                .get(slot)
                .ok_or("set_string_prototype: undefined source object")
        );

        self.memory_manager.set_string_prototype(object);

        Ok(())
    }

    fn ins_set_array_prototype(&self, thread: RcThread, _: RcCompiledCode,
                               instruction: &Instruction)
                               -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_array_prototype: missing object slot")
        );

        let object = try!(
            thread_ref.register()
                .get(slot)
                .ok_or("set_array_prototype: undefined source object")
        );

        self.memory_manager.set_array_prototype(object);

        Ok(())
    }

    fn ins_set_thread_prototype(&self, thread: RcThread, _: RcCompiledCode,
                                instruction: &Instruction)
                                -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_array_prototype: missing object slot")
        );

        let object = try!(
            thread.write()
                .unwrap()
                .register()
                .get(slot)
                .ok_or("set_array_prototype: undefined source object")
        );

        self.memory_manager.set_thread_prototype(object.clone());

        // Update the prototype of all existing threads (usually only the main
        // thread at this point).
        let threads = self.threads.read().unwrap();

        for thread in threads.iter() {
            thread.write().unwrap().set_prototype(object.clone());
        }

        Ok(())
    }

    fn ins_set_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let local_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_local: missing local variable index".to_string())
        );

        let object_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_local: missing object slot".to_string())
        );

        let object = try!(
            thread_ref.register()
                .get(object_index)
                .ok_or("set_local: undefined object".to_string())
        );

        thread_ref.variable_scope().insert(local_index, object);

        Ok(())
    }

    fn ins_get_local(&self, thread: RcThread, _: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("get_local: missing slot index".to_string())
        );

        let local_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("get_local: missing local variable index".to_string())
        );

        let object = try!(
            thread_ref.variable_scope()
                .get(local_index)
                .ok_or("get_local: undefined local variable index".to_string())
        );

        thread_ref.register().set(slot_index, object);

        Ok(())
    }

    fn ins_set_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let target_slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_const: missing target object slot".to_string())
        );

        let source_slot = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_const: missing source object slot".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("set_const: missing string literal index".to_string())
        );

        let target = try!(
            thread_ref.register()
                .get(target_slot)
                .ok_or("set_const: undefined target object".to_string())
        );

        let source = try!(
            thread_ref.register()
                .get(source_slot)
                .ok_or("set_const: undefined source object".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("set_const: undefined string literal".to_string())
        );

        target.write().unwrap().add_constant(name.clone(), source);

        Ok(())
    }

    fn ins_get_const(&self, thread: RcThread, code: RcCompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("get_const: missing slot index".to_string())
        );

        let src_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("get_const: missing source index".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("get_const: missing string literal index".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("get_const: undefined string literal".to_string())
        );

        let src = try!(
            thread_ref.register()
                .get(src_index)
                .ok_or("get_const: undefined source object".to_string())
        );

        let object = try!(
            src.read().unwrap()
                .lookup_constant(name)
                .ok_or(format!("get_const: Undefined constant {}", name))
        );

        thread_ref.register().set(index, object);

        Ok(())
    }

    fn ins_set_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let target_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_attr: missing target object slot".to_string())
        );

        let source_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("set_attr: missing source object slot".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("set_attr: missing string literal index".to_string())
        );

        let target_object = try!(
            thread_ref.register()
                .get(target_index)
                .ok_or("set_attr: undefined target object".to_string())
        );

        let source_object = try!(
            thread_ref.register()
                .get(source_index)
                .ok_or("set_attr: undefined target object".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("set_attr: undefined string literal".to_string())
        );

        target_object.write()
            .unwrap()
            .add_attribute(name.clone(), source_object);

        Ok(())
    }

    fn ins_get_attr(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let target_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("get_attr: missing target slot index".to_string())
        );

        let source_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("get_attr: missing source slot index".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("get_attr: missing string literal index".to_string())
        );

        let source = try!(
            thread_ref.register()
                .get(source_index)
                .ok_or("get_attr: undefined source object".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("get_attr: undefined string literal".to_string())
        );

        let attr = try!(
            source.read().unwrap()
                .lookup_attribute(name)
                .ok_or(format!("get_attr: undefined attribute \"{}\"", name))
        );

        thread_ref.register().set(target_index, attr);

        Ok(())
    }

    fn ins_send(&self, thread: RcThread, code: RcCompiledCode,
                instruction: &Instruction) -> Result<(), String> {
        let result_slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("send: missing result slot".to_string())
        );

        let receiver_slot = *try!(
            instruction.arguments
                .get(1)
                .ok_or("send: missing receiver slot".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("send: missing string literal index".to_string())
        );

        let allow_private = *try!(
            instruction.arguments
                .get(3)
                .ok_or("send: missing method visibility".to_string())
        );

        let arg_count = *try!(
            instruction.arguments
                .get(4)
                .ok_or("send: missing argument count".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("send: undefined string literal".to_string())
        );

        let receiver = try!(
            thread.write().unwrap()
                .register()
                .get(receiver_slot)
                .ok_or(format!(
                    "send: \"{}\" called on an undefined receiver",
                    name
                ))
        );

        let method_code = try!(
            receiver.read().unwrap()
                .lookup_method(name)
                .ok_or(receiver.read().unwrap().undefined_method_error(name))
        );

        if method_code.is_private() && allow_private == 0 {
            return Err(receiver.read().unwrap().private_method_error(name));
        }

        let mut arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 5, arg_count)
        );

        if arguments.len() != method_code.required_arguments {
            return Err(format!(
                "send: \"{}\" requires {} arguments, {} given",
                name,
                method_code.required_arguments,
                arguments.len()
            ));
        }

        // Expose the receiver as "self" to the method
        arguments.insert(0, receiver);

        let retval = try!(
            self.run_code(thread.clone(), method_code, arguments)
        );

        if retval.is_some() {
            thread.write().unwrap().register().set(result_slot, retval.unwrap());
        }

        Ok(())
    }

    fn ins_return(&self, thread: RcThread, _: RcCompiledCode,
                  instruction: &Instruction)
                  -> Result<Option<RcObject>, String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("return: missing return slot".to_string())
        );

        Ok(thread_ref.register().get(slot))
    }

    fn ins_goto_if_undef(&self, thread: RcThread, _: RcCompiledCode,
                         instruction: &Instruction)
                         -> Result<Option<usize>, String> {
        let mut thread_ref = thread.write().unwrap();

        let go_to = *try!(
            instruction.arguments
                .get(0)
                .ok_or("goto_if_undef: missing instruction index".to_string())
        );

        let value_slot = *try!(
            instruction.arguments
                .get(1)
                .ok_or("goto_if_undef: missing value slot".to_string())
        );

        let value   = thread_ref.register().get(value_slot);
        let matched = match value {
            Some(_) => { None },
            None    => { Some(go_to) }
        };

        Ok(matched)
    }

    fn ins_goto_if_def(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction)
                       -> Result<Option<usize>, String> {
        let mut thread_ref = thread.write().unwrap();

        let go_to = *try!(
            instruction.arguments
                .get(0)
                .ok_or("goto_if_def: missing instruction index".to_string())
        );

        let value_slot = *try!(
            instruction.arguments
                .get(1)
                .ok_or("goto_if_def: missing value slot".to_string())
        );

        let value   = thread_ref.register().get(value_slot);
        let matched = match value {
            Some(_) => { Some(go_to) },
            None    => { None }
        };

        Ok(matched)
    }

    fn ins_def_method(&self, thread: RcThread, code: RcCompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let receiver_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("def_method: missing receiver slot".to_string())
        );

        let name_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("def_method: missing string literal index".to_string())
        );

        let code_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("def_method: missing code object index".to_string())
        );

        let receiver = try!(
            thread_ref.register()
                .get(receiver_index)
                .ok_or("def_method: undefined receiver".to_string())
        );

        let name = try!(
            code.string_literals
                .get(name_index)
                .ok_or("def_method: undefined string literal".to_string())
        );

        let method_code = try!(
            code.code_objects
                .get(code_index)
                .cloned()
                .ok_or("def_method: undefined code object index".to_string())
        );

        let mut receiver_ref = receiver.write().unwrap();

        receiver_ref.add_method(name.clone(), method_code);

        Ok(())
    }

    fn ins_run_code(&self, thread: RcThread, code: RcCompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let result_index = *try!(
            instruction.arguments
                .get(0)
                .ok_or("run_code: missing result slot".to_string())
        );

        let code_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("run_code: missing code object index".to_string())
        );

        let arg_count = *try!(
            instruction.arguments
                .get(2)
                .ok_or("run_code: missing argument count".to_string())
        );

        let code_obj = try!(
            code.code_objects
                .get(code_index)
                .cloned()
                .ok_or("run_code: undefined code object".to_string())
        );

        let arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 3, arg_count)
        );

        let retval = try!(self.run_code(thread.clone(), code_obj, arguments));

        if retval.is_some() {
            thread.write()
                .unwrap()
                .register()
                .set(result_index, retval.unwrap());
        }

        Ok(())
    }

    fn ins_get_toplevel(&self, thread: RcThread, _: RcCompiledCode,
                        instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("get_toplevel: missing slot index")
        );

        let top_level = self.memory_manager.top_level.clone();

        thread_ref.register().set(slot, top_level);

        Ok(())
    }

    fn ins_add_integer(&self, thread: RcThread, _: RcCompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.write().unwrap();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("add_integer: missing target slot index".to_string())
        );

        let left_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("add_integer: missing left-hand slot index".to_string())
        );

        let right_index = *try!(
            instruction.arguments
                .get(2)
                .ok_or("add_integer: missing right-hand slot index".to_string())
        );

        let left_object = try!(
            thread_ref.register()
                .get(left_index)
                .ok_or("add_integer: undefined left-hand object".to_string())
        );

        let right_object = try!(
            thread_ref.register()
                .get(right_index)
                .ok_or("add_integer: undefined right-hand object".to_string())
        );

        let prototype = try!(
            self.memory_manager
                .integer_prototype()
                .ok_or("add_integer: no Integer prototype set up".to_string())
        );

        let left_ref  = left_object.read().unwrap();
        let right_ref = right_object.read().unwrap();

        if !left_ref.value.is_integer() || !right_ref.value.is_integer() {
            return Err(
                "add_integer: both objects must be integers".to_string()
            )
        }

        let added = left_ref.value.unwrap_integer() +
            right_ref.value.unwrap_integer();

        let obj = self.memory_manager
            .allocate(ObjectValue::Integer(added), prototype.clone());

        thread_ref.register().set(slot, obj);

        Ok(())
    }

    fn ins_start_thread(&self, thread: RcThread, code: RcCompiledCode,
                        instruction: &Instruction) -> Result<(), String> {
        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("start_thread: missing slot index".to_string())
        );

        let code_index = *try!(
            instruction.arguments
                .get(1)
                .ok_or("start_thread: missing code object index".to_string())
        );

        let thread_code = try!(
            code.code_objects
                .get(code_index)
                .cloned()
                .ok_or("start_thread: undefined code object".to_string())
        );

        try!(
            self.memory_manager
                .thread_prototype()
                .ok_or("start_thread: no Thread prototype set up".to_string())
        );

        let thread_object = self.run_thread(thread_code);

        thread.write().unwrap().register().set(slot, thread_object);

        Ok(())
    }

    fn print_error(&self, thread: RcThread, message: String) {
        let thread_ref = thread.read().unwrap();
        let mut stderr = io::stderr();
        let mut error  = message.to_string();

        thread_ref.call_frame().each_frame(|frame| {
            error.push_str(&format!(
                "\n{} line {} in \"{}\"",
                frame.file,
                frame.line,
                frame.name
            ));
        });

        write!(&mut stderr, "{}\n", error).unwrap();
    }

    fn run_code(&self, thread: RcThread, code: RcCompiledCode,
                args: Vec<RcObject>) -> Result<Option<RcObject>, String> {
        // Scoped so the borrow_mut is local to the block, allowing recursive
        // calling of the "run" method.
        {
            let mut thread_ref = thread.write().unwrap();

            thread_ref.push_call_frame(CallFrame::from_code(code.clone()));

            let mut variables = thread_ref.variable_scope();

            for arg in args.iter() {
                variables.add(arg.clone());
            }
        }

        let return_val = try!(self.run(thread.clone(), code));

        thread.write().unwrap().pop_call_frame();

        Ok(return_val)
    }

    fn collect_arguments(&self, thread: RcThread, instruction: &Instruction,
                         offset: usize,
                         amount: usize) -> Result<Vec<RcObject>, String> {
        let mut args: Vec<RcObject> = Vec::new();

        let mut thread_ref = thread.write().unwrap();

        for index in offset..(offset + amount) {
            let arg_index = instruction.arguments[index];

            let arg = try!(
                thread_ref.register()
                    .get(arg_index)
                    .ok_or(format!("argument {} is undefined", index))
            );

            args.push(arg)
        }

        Ok(args)
    }

    fn run_thread(&self, code: RcCompiledCode) -> RcObject {
        let self_clone = self.clone();
        let code_clone = code.clone();

        let (chan_sender, chan_receiver) = channel();

        let handle = thread::spawn(move || {
            let vm_thread: RcThread = chan_receiver.recv().unwrap();

            let result = self_clone.run(vm_thread.clone(), code_clone);

            // TODO: remove thread from self.threads

            match result {
                Ok(obj) => {
                    vm_thread.write().unwrap().value = obj;
                },
                Err(message) => {
                    self_clone.print_error(vm_thread, message);

                    // TODO: shut down all threads
                }
            };
        });

        let vm_thread  = Thread::from_code(code.clone(), Some(handle));
        let thread_obj = self.allocate_thread(vm_thread.clone());

        chan_sender.send(vm_thread.clone()).unwrap();

        thread_obj
    }

    fn allocate_thread(&self, thread: RcThread) -> RcObject {
        let thread_obj = self.memory_manager.allocate_thread(thread);

        self.threads.write().unwrap().push(thread_obj.clone());

        thread_obj
    }
}
