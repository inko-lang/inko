use std::io::{self, Write};

use compiled_code::CompiledCode;
use instruction::{InstructionType, Instruction};
use object::RcObject;
use thread::{Thread, RcThread};

/// Structure representing a single VM instance.
///
/// A VirtualMachine manages threads, runs instructions, starts/terminates
/// threads and so on. VirtualMachine instances are fully self contained
/// allowing multiple instances to run fully isolated in the same process.
///
pub struct VirtualMachine<'l> {
    /// All threads that are currently active.
    pub threads: Vec<RcThread<'l>>
}

impl<'l> VirtualMachine<'l> {
    /// Creates a new VirtualMachine without any threads.
    pub fn new() -> VirtualMachine<'l> {
        VirtualMachine {
            threads: Vec::new()
        }
    }

    /// Starts the main thread
    ///
    /// This requires a CompiledCode to run. Calling this method will block
    /// execution as the main thread is executed in the same OS thread as the
    /// caller of this function is operating in.
    ///
    pub fn start(&mut self, code: &CompiledCode) -> Result<(), ()> {
        let frame  = code.new_call_frame();
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
    pub fn run(&self, thread: RcThread<'l>,
               code: &CompiledCode) -> Result<Option<RcObject<'l>>, String> {
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
                InstructionType::SetObject => {
                    try!(self.ins_set_object(thread.clone(), code, &instruction));
                },
                InstructionType::SetInteger => {
                    try!(self.ins_set_integer(thread.clone(), code, &instruction));
                },
                InstructionType::SetFloat => {
                    try!(self.ins_set_float(thread.clone(), code, &instruction));
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

    /// Allocates and sets a regular object in a register slot.
    ///
    /// This instruction requires a single argument: the index of the slot to
    /// store the object in.
    ///
    /// # Examples
    ///
    ///     0: set_object 0
    ///
    pub fn ins_set_object(&self, thread: RcThread<'l>, _: &CompiledCode,
                          instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let index = *try!(
            instruction.arguments.get(0)
                .ok_or("set_object requires a single argument".to_string())
        );

        let value = thread_ref.young_heap().allocate_object();

        thread_ref.register().set(index, value);

        Ok(())
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
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///
    pub fn ins_set_integer(&self, thread: RcThread<'l>, code: &CompiledCode,
                           instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments.get(0)
                .ok_or("set_integer argument 1 is required".to_string())
        );

        let index = *try!(
            instruction.arguments.get(1)
                .ok_or("set_integer argument 2 is required".to_string())
        );

        let value = *try!(
            code.integer_literals.get(index)
                .ok_or("set_integer received an undefined literal".to_string())
        );

        let object = thread_ref.young_heap().allocate_integer(value);

        thread_ref.register().set(slot, object);

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
    ///     float_literals:
    ///       0: 10.5
    ///
    ///     0: set_float 0, 0
    ///
    pub fn ins_set_float(&self, thread: RcThread<'l>, code: &CompiledCode,
                         instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments.get(0)
                .ok_or("set_float argument 1 is required".to_string())
        );

        let index = *try!(
            instruction.arguments.get(1)
                .ok_or("set_float argument 2 is required".to_string())
        );

        let value  = *try!(
            code.float_literals.get(index)
                .ok_or("set_float received an undefined literal".to_string())
        );

        let object = thread_ref.young_heap().allocate_float(value);

        thread_ref.register().set(slot, object);

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
    ///     integer_literals:
    ///       0: 10
    ///       1: 20
    ///
    ///     string_literals:
    ///       0: "+"
    ///
    ///     0: set_integer 0, 0              # 10
    ///     1: set_integer 1, 1              # 20
    ///     2: send        2, 0, 0, 0, 1, 1  # 10.+(20)
    ///
    pub fn ins_send(&self, thread: RcThread<'l>, code: &CompiledCode,
                    instruction: &Instruction) -> Result<(), String> {
        let mut thread_ref = thread.borrow_mut();

        let result_slot = *try!(
            instruction.arguments.get(0)
                .ok_or("send argument 1 is required".to_string())
        );

        let receiver_slot = *try!(
            instruction.arguments.get(1)
                .ok_or("send argument 2 is required".to_string())
        );

        let name_index = *try!(
            instruction.arguments.get(2)
                .ok_or("send argument 3 is required".to_string())
        );

        let allow_private = *try!(
            instruction.arguments.get(3)
                .ok_or("send argument 4 is required".to_string())
        );

        let arg_count = *try!(
            instruction.arguments.get(4)
                .ok_or("send argument 5 is required".to_string())
        );

        let name = try!(
            code.string_literals.get(name_index)
                .ok_or("send received an undefined literal".to_string())
        );

        let receiver = try!(
            thread_ref.register().get(receiver_slot)
                .ok_or(format!(
                    "Attempt to call {} on an undefined receiver",
                    name
                ))
        );

        let mut receiver_ref = receiver.borrow_mut();

        let method_code = &try!(
            receiver_ref.lookup_method(name)
                .ok_or(format!(
                    "Undefined method \"{}\" called on {}",
                    name,
                    receiver_ref.name()
                ))
        );

        if method_code.is_private() && allow_private == 0 {
            return Err(format!(
                "Can't call private method \"{}\" on {}",
                name,
                receiver_ref.name()
            ));
        }

        let mut arguments: Vec<RcObject<'l>> = Vec::new();

        // First collect the arguments before we switch over to a new register
        for index in 5..(5 + arg_count) {
            let arg_index = instruction.arguments[index];

            let arg = try!(
                thread_ref.register().get(arg_index)
                    .ok_or(format!("send argument {} is undefined", index))
            );

            arguments.push(arg);
        }

        thread_ref.add_call_frame_from_compiled_code(code);

        // Now we can set the arguments in the new register
        for arg in arguments.iter() {
            thread_ref.variable_scope().add(arg.clone());
        }

        let return_val = try!(self.run(thread.clone(), method_code));

        if return_val.is_some() {
            thread_ref.register().set(result_slot, return_val.unwrap())
        }

        thread_ref.pop_call_frame();

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
    ///     integer_literals:
    ///       0: 10
    ///
    ///     0: set_integer 0, 0
    ///     1: return      0
    ///
    fn ins_return(&self, thread: RcThread<'l>, _: &CompiledCode,
                  instruction: &Instruction)
                  -> Result<Option<RcObject<'l>>, String> {
        let mut thread_ref = thread.borrow_mut();

        let slot = *try!(
            instruction.arguments.get(0)
                .ok_or("return argument 1 is required".to_string())
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
    ///     integer_literals:
    ///       0: 10
    ///       1: 20
    ///
    ///     0: goto_if_undef 0, 1
    ///     1: set_integer   0, 0
    ///     2: set_integer   0, 1
    ///
    /// Here slot "0" would be set to "20".
    ///
    pub fn ins_goto_if_undef(&self, thread: RcThread<'l>, _: &CompiledCode,
                             instruction: &Instruction)
                             -> Result<Option<usize>, String> {
        let mut thread_ref = thread.borrow_mut();

        let go_to = *try!(
            instruction.arguments.get(0)
                .ok_or("goto_if_undef argument 1 is required".to_string())
        );

        let value_slot = *try!(
            instruction.arguments.get(1)
                .ok_or("goto_if_undef argument 2 is required".to_string())
        );

        let value   = thread_ref.register().get(value_slot);
        let matched = match value {
            Some(_) => { None },
            None    => { Some(go_to) }
        };

        Ok(matched)
    }

    /// Prints a VM backtrace of a given thread with a message.
    fn print_error(&self, thread: RcThread<'l>, message: String) {
        let thread_ref = thread.borrow();
        let mut stderr = io::stderr();
        let mut error  = message.to_string();

        thread_ref.call_frame().each_frame(|frame| {
            error.push_str(
                &format!("\n{}:{} in {}", frame.file, frame.line, frame.name)
            );
        });

        write!(&mut stderr, "{}\n", error).unwrap();
    }
}
