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

    pub fn run(&self, thread: &mut Thread<'l>, code: &CompiledCode<'l>) {
        thread.add_call_frame_from_compiled_code(code);

        for instruction in &code.instructions {
            match instruction.instruction_type {
                InstructionType::SetInteger => {
                    self.set_integer(thread, &instruction);
                },
                _ => {
                    panic!("Unknown instruction type {:?}", instruction.instruction_type);
                }
            };
        }

        thread.pop_call_frame();
    }

    pub fn set_integer(&self, thread: &mut Thread, instruction: &Instruction) {
        let slot  = instruction.arguments[0];
        let value = instruction.arguments[1];

        thread.call_frame.register.set(slot, value);
    }
}
