use std::rc::Rc;
use std::cell::{RefCell, RefMut};
use std::io::{self, Write};
use std::process;

use compiled_code::CompiledCode;
use instruction::{InstructionType, Instruction};
use object::RcObject;
use thread::Thread;

/// Matches the option and returns the wrapped value if present, exiting the VM
/// otherwise.
///
/// # Examples
///
///     let option     = Option::None;
///     let thread_ref = thread.borrow_mut();
///
///     let value = some_or_terminate!(option, self, thread_ref, "Bummer!");
///
macro_rules! some_or_terminate {
    ($value: expr, $vm: expr, $thread: expr, $message: expr) => {
        match $value {
            Option::Some(wrapped) => {
                wrapped
            },
            Option::None => {
                $vm.terminate_vm(&$thread, $message);
            }
        }
    }
}

/// A mutable, reference counted Thread.
pub type RcThread<'l> = Rc<RefCell<Thread<'l>>>;

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
    pub fn start(&mut self, code: &CompiledCode) {
        let frame  = code.new_call_frame();
        let thread = Rc::new(RefCell::new(Thread::new(frame)));

        self.threads.push(thread.clone());

        self.run(thread, code);
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
    pub fn run(&self, thread: RcThread<'l>, code: &CompiledCode) -> Option<RcObject<'l>> {
        let mut skip_until: Option<usize> = Option::None;
        let mut retval = Option::None;

        for (index, instruction) in code.instructions.iter().enumerate() {
            if skip_until.is_some() {
                if index < skip_until.unwrap() {
                    continue;
                }
                else {
                    skip_until = Option::None;
                }
            }

            match instruction.instruction_type {
                InstructionType::SetObject => {
                    self.ins_set_object(thread.clone(), code, &instruction);
                },
                InstructionType::SetInteger => {
                    self.ins_set_integer(thread.clone(), code, &instruction);
                },
                InstructionType::SetFloat => {
                    self.ins_set_float(thread.clone(), code, &instruction);
                },
                InstructionType::Send => {
                    self.ins_send(thread.clone(), code, &instruction);
                },
                InstructionType::Return => {
                    retval = self.ins_return(thread.clone(), code, &instruction);
                },
                InstructionType::GotoIfUndef => {
                    skip_until = self
                        .ins_goto_if_undef(thread.clone(), code, &instruction);
                },
                _ => {
                    let thread_ref = thread.borrow_mut();

                    self.terminate_vm(
                        &thread_ref,
                        format!(
                            "Unknown instruction \"{:?}\"",
                            instruction.instruction_type
                        )
                    );
                }
            };
        }

        retval
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
                          instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot  = instruction.arguments[0];
        let value = thread_ref.young_heap().allocate_object();

        thread_ref.register().set(slot, value);
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
                           instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.integer_literals[index];
        let object = thread_ref.young_heap().allocate_integer(value);

        thread_ref.register().set(slot, object);
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
                         instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.float_literals[index];
        let object = thread_ref.young_heap().allocate_float(value);

        thread_ref.register().set(slot, object);
    }

    /// Sends a message and stores the result in a register slot.
    ///
    /// This instruction requires at least 4 arguments:
    ///
    /// 1. The slot index to store the result in.
    /// 2. The slot index of the receiver.
    /// 3. The index of the string literals to use for the method name.
    /// 4. The amount of arguments to pass (0 or more).
    ///
    /// If the argument amount is set to N where N > 0 then the N instruction
    /// arguments following the 4th instruction argument are used as arguments
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
    ///     0: set_integer 0, 0           # 10
    ///     1: set_integer 1, 1           # 20
    ///     2: send        2, 0, 0, 1, 1  # 10.+(20)
    ///
    pub fn ins_send(&self, thread: RcThread<'l>, code: &CompiledCode,
                    instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let result_slot   = instruction.arguments[0];
        let receiver_slot = instruction.arguments[1];
        let name_index    = instruction.arguments[2];
        let arg_count     = instruction.arguments[3];

        let name = some_or_terminate!(
            code.string_literals.get(name_index),
            self,
            thread_ref,
            format!("No method name literal defined at index {}", name_index)
        );

        let receiver = some_or_terminate!(
            thread_ref.register().get(receiver_slot),
            self,
            thread_ref,
            format!("Attempt to call {} on an undefined receiver", name)
        );

        let mut receiver_ref = receiver.borrow_mut();

        let method_code = &some_or_terminate!(
            receiver_ref.lookup_method(name),
            self,
            thread_ref,
            format!("Undefined method \"{}\" called on an object", name)
        );

        let mut arguments: Vec<RcObject<'l>> = Vec::new();

        // First collect the arguments before we switch over to a new register
        for index in 4..(4 + arg_count) {
            let arg_index = instruction.arguments[index];

            let arg = some_or_terminate!(
                thread_ref.register().get(arg_index),
                self,
                thread_ref,
                "Attempt to use an undefined value as an argument".to_string()
            );

            arguments.push(arg);
        }

        thread_ref.add_call_frame_from_compiled_code(code);

        // Now we can set the arguments in the new register
        for arg in arguments.iter() {
            thread_ref.variable_scope().add(arg.clone());
        }

        let return_val = self.run(thread.clone(), method_code);

        if return_val.is_some() {
            thread_ref.register().set(result_slot, return_val.unwrap())
        }

        thread_ref.pop_call_frame();
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
                  instruction: &Instruction) -> Option<RcObject<'l>> {
        let mut thread_ref = thread.borrow_mut();

        let slot = instruction.arguments[0];

        thread_ref.register().get(slot)
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
                             instruction: &Instruction) -> Option<usize> {
        let mut thread_ref = thread.borrow_mut();

        let go_to      = instruction.arguments[0];
        let value_slot = instruction.arguments[1];
        let value      = thread_ref.register().get(value_slot);

        match value {
            Option::Some(_) => { Option::None },
            Option::None    => { Option::Some(go_to) }
        }
    }

    /// Prints a VM backtrace and terminates the current VM instance.
    ///
    /// This should only be used for serious errors that can't be handled in any
    /// reasonable way.
    ///
    fn terminate_vm(&self, thread: &RefMut<Thread<'l>>, message: String) -> ! {
        let mut stderr = io::stderr();
        let mut error  = message.to_string();

        thread.call_frame().each_frame(|frame| {
            error.push_str(
                &format!("\n{}:{} in {}", frame.file, frame.line, frame.name)
            );
        });

        write!(&mut stderr, "{}\n", error).unwrap();

        // TODO: shut down threads properly
        // TODO: replace exit with something that doesn't kill the entire process
        process::exit(1);
    }
}
