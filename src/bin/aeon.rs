extern crate libaeon;

use std::sync::Arc;
use std::process;

use libaeon::virtual_machine::{VirtualMachine, ArcMethods};
use libaeon::compiled_code::CompiledCode;
use libaeon::instruction::{InstructionType, Instruction};

fn main() {
    let vm = VirtualMachine::new();
    let mut cc = CompiledCode::new(
        "<main>".to_string(),
        "/tmp/test.rs".to_string(),
        1,
        vec![
            Instruction::new(InstructionType::SetObject, vec![0], 1, 1),
            Instruction::new(InstructionType::SetIntegerPrototype, vec![0], 1, 1),
            Instruction::new(InstructionType::SetName, vec![0, 1], 1, 1),
            Instruction::new(InstructionType::SetObject, vec![1], 1, 1),
            Instruction::new(InstructionType::DefMethod, vec![1, 0, 0], 2, 1),
            Instruction::new(InstructionType::Send, vec![2, 1, 0, 0, 0], 3, 1),
            Instruction::new(InstructionType::Return, vec![2], 3, 1)
        ]
    );

    cc.add_string_literal("+".to_string());
    cc.add_string_literal("Integer".to_string());

    let mut method_code = CompiledCode::new(
        "test".to_string(),
        "/tmp/test.rs".to_string(),
        2,
        vec![
            Instruction::new(InstructionType::SetInteger, vec![0, 0], 3, 1),
            Instruction::new(InstructionType::SetInteger, vec![1, 1], 3, 5),
            Instruction::new(InstructionType::IntegerAdd, vec![2, 0, 1], 3, 3),
            Instruction::new(InstructionType::Return, vec![2], 3, 1)
        ]
    );

    method_code.add_integer_literal(10);
    method_code.add_integer_literal(20);

    cc.add_code_object(Arc::new(method_code));

    let result = vm.start(Arc::new(cc));

    if result.is_err() {
        process::exit(1);
    }
}
