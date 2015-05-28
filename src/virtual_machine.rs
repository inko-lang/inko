use compiled_code::CompiledCode;
use heap::Heap;
use instruction::{InstructionType, Instruction};
use thread::Thread;

pub struct VirtualMachine<'l> {
    threads: Vec<Thread<'l>>,
    global_heap: Heap<'l>
}

impl<'l> VirtualMachine<'l> {
    pub fn new() -> VirtualMachine<'l> {
        VirtualMachine {
            threads: Vec::new(),
            global_heap: Heap::new()
        }
    }

    pub fn run(&self, thread: &mut Thread<'l>, code: &CompiledCode) {
        thread.add_call_frame_from_compiled_code(code);

        for instruction in &code.instructions {
            match instruction.instruction_type {
                InstructionType::SetInteger => {
                    self.set_integer(thread, code, &instruction);
                },
                InstructionType::SetFloat => {
                    self.set_float(thread, code, &instruction);
                },
                _ => {
                    panic!(
                        "Unknown instruction type {:?}",
                        instruction.instruction_type
                    );
                }
            };
        }

        thread.pop_call_frame();
    }

    pub fn set_integer(&self, thread: &mut Thread<'l>, code: &CompiledCode,
                       instruction: &Instruction) {
        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.integer_literals[index];
        let object = thread.young_heap().allocate_integer(value);

        thread.register().set(slot, object);
    }

    pub fn set_float(&self, thread: &mut Thread<'l>, code: &CompiledCode,
                     instruction: &Instruction) {
        let slot   = instruction.arguments[0];
        let index  = instruction.arguments[1];
        let value  = code.float_literals[index];
        let object = thread.young_heap().allocate_float(value);

        thread.register().set(slot, object);
    }

    pub fn send(&self, thread: &mut Thread<'l>, code: &CompiledCode,
                instruction: &Instruction) {
        let result_slot   = instruction.arguments[0];
        let receiver_slot = instruction.arguments[1];
        let name_index    = instruction.arguments[2];
        let arg_count     = instruction.arguments[3];

        let receiver  = thread.register().get(receiver_slot);
        let ref name  = code.string_literals[name_index];

        if ( !receiver.methods.contains_key(name) ) {
            // TODO: make this a proper VM error with a backtrace
            panic!("Undefined method {}", name);
        }

        let ref method_code = receiver.methods[name];

        self.run(thread, method_code);
    }
}
