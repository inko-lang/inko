//! Functions for testing instruction handlers.
use crate::block::Block;
use crate::compiled_code::CompiledCode;
use crate::config::Config;
use crate::process::RcProcess;
use crate::vm::instruction::{Instruction, InstructionType};
use crate::vm::machine::Machine;
use crate::vm::process;
use crate::vm::state::State;

/// Sets up a VM with a single process.
pub fn setup() -> (Machine, Block, RcProcess) {
    let mut config = Config::new();

    config.primary_threads = 2;
    config.blocking_threads = 2;
    config.gc_threads = 2;
    config.tracer_threads = 2;

    let state = State::with_rc(config, &[]);
    let name = state.intern_string("a".to_string());
    let machine = Machine::default(state);
    let mut code = CompiledCode::new(
        name,
        name,
        1,
        vec![new_instruction(InstructionType::Return, vec![0])],
    );

    // Reserve enough space for registers/locals for most tests.
    code.locals = 32;
    code.registers = 1024;

    let (block, process) = {
        let mut registry = machine.module_registry.lock();
        let module_path = if cfg!(windows) { "C:\\test" } else { "/test" };
        let module_ptr = registry.define_module("test", module_path, code);
        let module = module_ptr.module_value().unwrap();
        let scope = module.global_scope_ref();
        let block = Block::new(module.code(), None, module_ptr, scope);
        let process = process::allocate(&machine.state, &block);

        process.set_main();

        (block, process)
    };

    (machine, block, process)
}

/// Creates a new instruction.
pub fn new_instruction(
    ins_type: InstructionType,
    args: Vec<u16>,
) -> Instruction {
    Instruction::new(ins_type, args, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::instruction::InstructionType;

    #[test]
    fn test_new_instruction() {
        let ins = new_instruction(InstructionType::SetLiteral, vec![1, 2]);

        assert_eq!(ins.instruction_type, InstructionType::SetLiteral);
        assert_eq!(ins.arguments, vec![1, 2]);
        assert_eq!(ins.line, 1);
    }
}
