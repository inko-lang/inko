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
    let vm    = VirtualMachine::new();
    let ins   = Instruction::new(InstructionType::SetInteger, vec![0, 1], 1, 1);
    let frame = CallFrame::new("main", "(eval)", 1);

    let mut thread = Thread::new(frame);

    let cc = CompiledCode {
        name: "main",
        file: "(eval)",
        line: 1,
        required_arguments: 0,
        optional_arguments: 0,
        rest_argument: false,
        locals: Vec::new(),
        instructions: vec![ins]
    };

    vm.run(&mut thread, &cc);
}
