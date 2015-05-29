use std::rc::Rc;
use std::cell::RefCell;

use compiled_code::CompiledCode;
use heap::Heap;
use instruction::{InstructionType, Instruction};
use object::RcObject;
use thread::Thread;

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

    pub fn run(&self, thread: RcThread<'l>, code: &CompiledCode) {
        for instruction in &code.instructions {
            match instruction.instruction_type {
                InstructionType::SetInteger => {
                    self.set_integer(thread.clone(), code, &instruction);
                },
                InstructionType::SetFloat => {
                    self.set_float(thread.clone(), code, &instruction);
                },
                InstructionType::Send => {
                    self.send(thread.clone(), code, &instruction);
                },
                _ => {
                    panic!(
                        "Unknown instruction type {:?}",
                        instruction.instruction_type
                    );
                }
            };
        }
    }

    pub fn set_integer(&self, thread: RcThread<'l>, code: &CompiledCode,
                       instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.integer_literals[index];
        let object = thread_ref.young_heap().allocate_integer(value);

        thread_ref.register().set(slot, object);
    }

    pub fn set_float(&self, thread: RcThread<'l>, code: &CompiledCode,
                     instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.float_literals[index];
        let object = thread_ref.young_heap().allocate_float(value);

        thread_ref.register().set(slot, object);
    }

    pub fn send(&self, thread: RcThread<'l>, code: &CompiledCode,
                instruction: &Instruction) {
        let mut thread_ref = thread.borrow_mut();

        let result_slot   = instruction.arguments[0];
        let receiver_slot = instruction.arguments[1];
        let name_index    = instruction.arguments[2];
        let arg_count     = instruction.arguments[3];

        let receiver  = thread_ref.register().get(receiver_slot);
        let ref name  = code.string_literals[name_index];

        if !receiver.methods.contains_key(name) {
            // TODO: make this a proper VM error with a backtrace
            panic!("Undefined method {}", name);
        }

        let ref method_code = receiver.methods[name];
        let mut arguments: Vec<RcObject<'l>> = Vec::new();

        // First collect the arguments before we switch over to a new register
        for index in 4..(4 + arg_count) {
            let arg_index = instruction.arguments[index];

            arguments.push(thread_ref.register().get(arg_index));
        }

        thread_ref.add_call_frame_from_compiled_code(code);

        // Now we can set the arguments in the new register
        for (index, arg) in arguments.iter().enumerate() {
            thread_ref.register().set(index, arg.clone());
        }

        // TODO: handle return values

        self.run(thread.clone(), method_code);

        thread_ref.pop_call_frame();
    }
}
