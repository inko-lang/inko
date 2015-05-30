extern crate libaeon;

use std::process;

use libaeon::virtual_machine::VirtualMachine;
use libaeon::compiled_code::CompiledCode;
use libaeon::instruction::{InstructionType, Instruction};

fn main() {
    let mut vm = VirtualMachine::new();
    let mut cc = CompiledCode::new(
        "<main>".to_string(),
        "(eval)".to_string(),
        1,
        vec![
            Instruction::new(InstructionType::SetInteger, vec![0, 0], 1, 1),
            Instruction::new(InstructionType::Send, vec![1, 0, 0, 0], 1, 1)
        ]
    );

    cc.add_integer_literal(10);
    cc.add_integer_literal(20);

    cc.add_string_literal("to_s".to_string());

    let result = vm.start(&cc);

    if result.is_err() {
        process::exit(1);
    }
}
