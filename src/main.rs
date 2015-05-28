// FIXME: re-enable once all code is actually used.
#![allow(dead_code)]

mod gc {
    mod baker;
    mod immix;
}

mod call_frame;
mod compiled_code;
mod heap;
mod instruction;
mod object;
mod register;
mod thread;
mod virtual_machine;
mod variable_scope;

use virtual_machine::VirtualMachine;
use call_frame::CallFrame;
use compiled_code::CompiledCode;
use instruction::{InstructionType, Instruction};
use thread::Thread;

fn main() {
    let name = "main".to_string();
    let file = "(eval)".to_string();

    let vm    = VirtualMachine::new();
    let ins   = Instruction::new(InstructionType::SetInteger, vec![0, 1], 1, 1);
    let frame = CallFrame::new(name.clone(), file.clone(), 1);
    let cc    = CompiledCode::new(name.clone(), file.clone(), 1, vec![ins]);

    let mut thread = Thread::new(frame);

    vm.run(&mut thread, &cc);
}
