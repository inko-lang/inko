extern crate libaeon;

use std::sync::Arc;
use std::process;

use libaeon::virtual_machine::VirtualMachine;
use libaeon::virtual_machine_methods::VirtualMachineMethods;
use libaeon::compiled_code::CompiledCode;
use libaeon::instruction::{InstructionType, Instruction};

fn main() {
    let vm = VirtualMachine::new();
    let mut cc = CompiledCode::new(
        "<main>".to_string(),
        "/tmp/test.rs".to_string(),
        1,
        vec![
            Instruction::new(InstructionType::SetString, vec![0, 0], 1, 1),
            Instruction::new(InstructionType::StdoutWrite, vec![1, 0], 1, 1)
        ]
    );

    cc.add_string_literal("hello world\n".to_string());

    let result = vm.start(Arc::new(cc));

    if result.is_err() {
        process::exit(1);
    }
}
