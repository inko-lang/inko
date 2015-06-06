use std::io::{self, Write};
use std::sync::RwLock;

use call_frame::CallFrame;
use compiled_code::CompiledCode;
use heap::Heap;
use instruction::{InstructionType, Instruction};
use object::{Object, ObjectValue, RcObject};
use thread::{Thread, RcThread};

/// Structure representing a single VM instance.
///
/// A VirtualMachine manages threads, runs instructions, starts/terminates
/// threads and so on. VirtualMachine instances are fully self contained
/// allowing multiple instances to run fully isolated in the same process.
///
pub struct VirtualMachine {
    // All threads that are currently active.
    threads: Vec<RcThread>,

    // The global heap is used for allocating constants and other objects that
    // usually stick around for a program's entire lifetime, regardless of what
    // thread created the data.
    global_heap: RwLock<Heap>,

    // The top-level object used for storing global constants.
    top_level: RcObject,

    // Various core objects that can be created using specialized instructions.
    integer_object: Option<RcObject>,
    float_object: Option<RcObject>,
    string_object: Option<RcObject>,
    array_object: Option<RcObject>,
}

impl VirtualMachine {
    /// Creates a new VirtualMachine without any threads.
    ///
    /// This also takes care of setting up the basic layout of the various core
    /// classes.
    ///
    pub fn new() -> VirtualMachine {
        let mut heap  = Heap::new();
        let top_level = Object::with_rc(ObjectValue::None);

        top_level.borrow_mut().pin();

        heap.store_object(top_level.clone());

        VirtualMachine {
            threads: Vec::new(),
            global_heap: RwLock::new(heap),
            top_level: top_level,
            integer_object: None,
            float_object: None,
            string_object: None,
            array_object: None
        }
    }

    /// Starts the main thread
    ///
    /// This requires a CompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    ///
    pub fn start(&mut self, code: &CompiledCode) -> Result<(), ()> {
        let frame  = CallFrame::from_code(code);
        let thread = Thread::with_rc(frame);

        self.threads.push(thread.clone());

        let result = self.run(thread.clone(), code);

        return match result {
            Ok(_)        => Ok(()),
            Err(message) => {
                self.print_error(thread, message);

                // TODO: shut down threads

                Err(())
            }
        }
    }

    /// Runs a CompiledCode for a specific Thread.
    ///
    /// This iterates over all instructions in the CompiledCode, executing them
    /// one by one (except when certain instructions dictate otherwise).
    ///
    /// The return value is whatever the last CompiledCode returned (if
    /// anything). Values are only returned when a CompiledCode ends with a
    /// "return" instruction.
    ///
    pub fn run(&mut self, thread: RcThread,
               code: &CompiledCode) -> Result<Option<RcObject>, String> {
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
                    try!(
                        self.ins_set_integer(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::SetFloat => {
                    try!(
                        self.ins_set_float(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::SetString => {
                    try!(
                        self.ins_set_string(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::SetObject => {
                    try!(
                        self.ins_set_object(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::SetName => {
                    try!(self.ins_set_name(thread.clone(), code, &instruction));
                },
                InstructionType::SetIntegerPrototype => {
                    try!(
                        self.ins_set_integer_prototype(
                            thread.clone(),
                            code,
                            &instruction
                        )
                    );
                },
                InstructionType::SetFloatPrototype => {
                    try!(
                        self.ins_set_float_prototype(
                            thread.clone(),
                            code,
                            &instruction
                        )
                    );
                },
                InstructionType::SetStringPrototype => {
                    try!(
                        self.ins_set_string_prototype(
                            thread.clone(),
                            code,
                            &instruction
                        )
                    );
                },
                InstructionType::GetConst => {
                    try!(
                        self.ins_get_const(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::Send => {
                    try!(self.ins_send(thread.clone(), code, &instruction));
                },
                InstructionType::Return => {
                    retval = try!(
                        self.ins_return(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::GotoIfUndef => {
                    skip_until = try!(
                        self.ins_goto_if_undef(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::GotoIfDef => {
                    skip_until = try!(
                        self.ins_goto_if_def(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::DefMethod => {
                    try!(
                        self.ins_def_method(thread.clone(), code, &instruction)
                    );
                },
                InstructionType::RunCode => {
                    try!(self.ins_run_code(thread.clone(), code, &instruction));
                },
                InstructionType::GetToplevel => {
                    try!(
                        self.ins_get_toplevel(thread.clone(), code, &instruction)
                    );
                },
                _ => {
                    return Err(format!(
                        "Unknown instruction \"{:?}\"",
                        instruction.instruction_type
                    ));
                }
            };
        }

        Ok(retval)
    }

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
    fn ins_set_integer(&mut self, thread: RcThread, code: &CompiledCode,
                       instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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
            self.integer_object
                .as_ref()
                .ok_or("set_integer: no Integer prototype set up".to_string())
        );

        let obj = Object::with_rc(ObjectValue::Integer(value));

        obj.borrow_mut().set_prototype(prototype.clone());

        thread_ref.young_heap().store_object(obj.clone());
        thread_ref.register().set(slot, obj);

        Ok(())
    }

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
    fn ins_set_float(&mut self, thread: RcThread, code: &CompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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
            self.float_object
                .as_ref()
                .ok_or("set_float: no Float prototype set up".to_string())
        );

        let obj = Object::with_rc(ObjectValue::Float(value));

        obj.borrow_mut().set_prototype(prototype.clone());

        thread_ref.young_heap().store_object(obj.clone());
        thread_ref.register().set(slot, obj);

        Ok(())
    }

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
    fn ins_set_string(&mut self, thread: RcThread, code: &CompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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
            self.string_object
                .as_ref()
                .ok_or("set_string: no String prototype set up".to_string())
        );

        let obj = Object::with_rc(ObjectValue::String(value.clone()));

        obj.borrow_mut().set_prototype(prototype.clone());

        thread_ref.young_heap().store_object(obj.clone());
        thread_ref.register().set(slot, obj);

        Ok(())
    }

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
    fn ins_set_object(&mut self, thread: RcThread, _: &CompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("set_objet: missing slot index".to_string())
        );

        let proto_index_opt = instruction.arguments.get(1);

        let obj = Object::with_rc(ObjectValue::None);

        if proto_index_opt.is_some() {
            let proto_index = *proto_index_opt.unwrap();

            let proto = try!(
                thread_ref.register()
                    .get(proto_index)
                    .ok_or("set_object: prototype is undefined".to_string())
            );

            obj.borrow_mut().set_prototype(proto);
        }

        thread_ref.young_heap().store_object(obj.clone());
        thread_ref.register().set(slot, obj);

        Ok(())
    }

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
    fn ins_set_name(&mut self, thread: RcThread, code: &CompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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

        obj.borrow_mut().set_name(name.clone());

        Ok(())
    }

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
    fn ins_set_integer_prototype(&mut self, thread: RcThread,
                                 _: &CompiledCode,
                                 instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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

        self.integer_object = Some(object.clone());

        Ok(())
    }

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
    fn ins_set_float_prototype(&mut self, thread: RcThread,
                               _: &CompiledCode,
                               instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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

        self.float_object = Some(object.clone());

        Ok(())
    }

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
    fn ins_set_string_prototype(&mut self, thread: RcThread,
                                _: &CompiledCode,
                                instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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

        self.string_object = Some(object.clone());

        Ok(())
    }

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
    fn ins_get_const(&mut self, thread: RcThread, code: &CompiledCode,
                     instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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
            src.borrow()
                .lookup_constant(name)
                .ok_or(format!("get_const: Undefined constant {}", name))
        );

        thread_ref.register().set(index, object);

        Ok(())
    }

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
    fn ins_send(&mut self, thread: RcThread, code: &CompiledCode,
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
            thread.borrow_mut()
                .register()
                .get(receiver_slot)
                .ok_or(format!(
                    "send: \"{}\" called on an undefined receiver",
                    name
                ))
        );

        let receiver_ref = receiver.borrow_mut();

        let method_code = &try!(
            receiver_ref.lookup_method(name)
                .ok_or(receiver_ref.undefined_method_error(name))
        );

        if method_code.is_private() && allow_private == 0 {
            return Err(receiver_ref.private_method_error(name));
        }

        let arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 5, arg_count)
        );

        let retval = try!(
            self.run_code(thread.clone(), method_code, arguments)
        );

        if retval.is_some() {
            thread.borrow_mut().register().set(result_slot, retval.unwrap());
        }

        Ok(())
    }

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
    fn ins_return(&mut self, thread: RcThread, _: &CompiledCode,
                  instruction: &Instruction)
                  -> Result<Option<RcObject>, String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("return: missing return slot".to_string())
        );

        Ok(thread_ref.register().get(slot))
    }

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
    fn ins_goto_if_undef(&mut self, thread: RcThread, _: &CompiledCode,
                         instruction: &Instruction)
                         -> Result<Option<usize>, String> {
        let mut thread_ref = thread.borrow_mut();

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
    fn ins_goto_if_def(&mut self, thread: RcThread, _: &CompiledCode,
                       instruction: &Instruction)
                       -> Result<Option<usize>, String> {
        let mut thread_ref = thread.borrow_mut();

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

    /// Defines a method for an object.
    ///
    /// This instruction requires 3 arguments:
    ///
    /// 1. A slot index pointing to a specific object to define the method on.
    /// 2. The string literal index containing the method name.
    /// 3. The code object index containing the CompiledCode of the method.
    ///
    fn ins_def_method(&mut self, thread: RcThread, code: &CompiledCode,
                      instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

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
                .ok_or("def_method: undefined code object index".to_string())
        );

        let mut receiver_ref = receiver.borrow_mut();

        receiver_ref.add_method(name.clone(), method_code.to_rc());

        Ok(())
    }

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
    fn ins_run_code(&mut self, thread: RcThread, code: &CompiledCode,
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
                .ok_or("run_code: undefined code object".to_string())
        );

        let arguments = try!(
            self.collect_arguments(thread.clone(), instruction, 3, arg_count)
        );

        let retval = try!(self.run_code(thread.clone(), code_obj, arguments));

        if retval.is_some() {
            thread.borrow_mut().register().set(result_index, retval.unwrap())
        }

        Ok(())
    }

    /// Sets the top-level object in a register slot.
    ///
    /// This instruction requires one argument: the slot to store the object in.
    ///
    /// # Examples
    ///
    ///     get_toplevel 0
    ///
    fn ins_get_toplevel(&mut self, thread: RcThread, _: &CompiledCode,
                        instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments
                .get(0)
                .ok_or("get_toplevel: missing slot index")
        );

        thread_ref.register().set(slot, self.top_level.clone());

        Ok(())
    }

    /// Prints a VM backtrace of a given thread with a message.
    fn print_error(&mut self, thread: RcThread, message: String) {
        let thread_ref = thread.borrow();
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

    /// Runs a given CompiledCode with arguments.
    fn run_code(&mut self, thread: RcThread, code: &CompiledCode,
                args: Vec<RcObject>) -> Result<Option<RcObject>, String> {
        // Scoped so the borrow_mut is local to the block, allowing recursive
        // calling of the "run" method.
        {
            let mut thread_ref = thread.borrow_mut();

            thread_ref.push_call_frame(CallFrame::from_code(code));

            let mut variables = thread_ref.variable_scope();

            for arg in args.iter() {
                variables.add(arg.clone());
            }
        }

        let return_val = try!(self.run(thread.clone(), code));

        thread.borrow_mut().pop_call_frame();

        Ok(return_val)
    }

    /// Collects a set of arguments from an instruction.
    fn collect_arguments(&mut self, thread: RcThread, instruction: &Instruction,
                         offset: usize,
                         amount: usize) -> Result<Vec<RcObject>, String> {
        let mut args: Vec<RcObject> = Vec::new();

        let mut thread_ref = thread.borrow_mut();

        for index in offset..(offset + amount) {
            let arg_index = instruction.arguments[index];

            let arg = try!(
                thread_ref
                    .register()
                    .get(arg_index)
                    .ok_or(format!("argument {} is undefined", index))
            );

            args.push(arg)
        }

        Ok(args)
    }
}
