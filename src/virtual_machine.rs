use std::rc::Rc;
use std::cell::{RefCell, RefMut};
use std::io::{self, Write};
use std::process;

use compiled_code::CompiledCode;
use heap::Heap;
use instruction::{InstructionType, Instruction};
use object::RcObject;
use thread::Thread;

/// Matches the option and returns the wrapped value if present, exits with a VM
/// error when it's not.
///
/// Example:
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

pub type RcThread<'l> = Rc<RefCell<Thread<'l>>>;

pub struct VirtualMachine<'l> {
    threads: Vec<RcThread<'l>>,
    global_heap: Heap<'l>
}

impl<'l> VirtualMachine<'l> {
    pub fn new() -> VirtualMachine<'l> {
        VirtualMachine {
            threads: Vec::new(),
            global_heap: Heap::new()
        }
    }

    pub fn start(&mut self, code: &CompiledCode) {
        let frame  = code.new_call_frame();
        let thread = Rc::new(RefCell::new(Thread::new(frame)));

        self.threads.push(thread.clone());

        self.run(thread, code);
    }

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

    fn ins_set_integer(&self, thread: RcThread<'l>, code: &CompiledCode,
                       instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.integer_literals[index];
        let object = thread_ref.young_heap().allocate_integer(value);

        thread_ref.register().set(slot, object);
    }

    fn ins_set_float(&self, thread: RcThread<'l>, code: &CompiledCode,
                     instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.float_literals[index];
        let object = thread_ref.young_heap().allocate_float(value);

        thread_ref.register().set(slot, object);
    }

    fn ins_send(&self, thread: RcThread<'l>, code: &CompiledCode,
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

        let ref method_code = some_or_terminate!(
            receiver.methods.get(name),
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

    fn ins_return(&self, thread: RcThread<'l>, code: &CompiledCode,
                  instruction: &Instruction) -> Option<RcObject<'l>> {
        let mut thread_ref = thread.borrow_mut();

        let slot = instruction.arguments[0];

        thread_ref.register().get(slot)
    }

    fn ins_goto_if_undef(&self, thread: RcThread<'l>, code: &CompiledCode,
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

        process::exit(1);
    }
}
