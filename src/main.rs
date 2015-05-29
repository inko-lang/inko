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
use compiled_code::CompiledCode;
use instruction::{InstructionType, Instruction};

fn main() {
    let mut vm = VirtualMachine::new();
    let mut cc = CompiledCode::new(
        "main".to_string(),
        "(eval)".to_string(),
        1,
        vec![
            Instruction::new(InstructionType::SetInteger, vec![0, 0], 1, 1),
            Instruction::new(InstructionType::Send, vec![1, 0, 0, 0], 1, 1)
        ]
    );

    cc.add_integer_literal(10);
    cc.add_string_literal("to_s".to_string());

    vm.start(&cc);
}
