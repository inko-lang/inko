//! Functions for testing instruction handlers.
use block::Block;
use compiled_code::CompiledCode;
use config::Config;
use module::Module;
use process::RcProcess;
use vm::instruction::{Instruction, InstructionType};
use vm::machine::Machine;
use vm::process;
use vm::state::State;

/// Sets up a VM with a single process.
pub fn setup() -> (Machine, Block, RcProcess) {
    let state = State::new(Config::new(), &[]);
    let name = state.intern_string("a".to_string());
    let machine = Machine::default(state);
    let mut code = CompiledCode::new(name, name, 1, Vec::new());

    // Reserve enough space for registers/locals for most tests.
    code.locals = 32;
    code.registers = 1024;

    let (block, process) = {
        let mut registry = machine.module_registry.lock();
        let module_name = if cfg!(windows) { "C:\\test" } else { "/test" };

        // To ensure the module sticks around long enough we'll manually store in
        // in the module registry.
        registry.add_module(module_name, Module::new(code));

        let lookup = registry
            .get_or_set(&module_name)
            .map_err(|err| err.message())
            .unwrap();

        let module = lookup.module;
        let scope = module.global_scope_ref();
        let block =
            Block::new(module.code(), None, machine.state.top_level, scope);
        let process = process::allocate(&machine.state, 0, &block);

        (block, process.unwrap())
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
    use vm::instruction::InstructionType;

    #[test]
    fn test_new_instruction() {
        let ins = new_instruction(InstructionType::SetLiteral, vec![1, 2]);

        assert_eq!(ins.instruction_type, InstructionType::SetLiteral);
        assert_eq!(ins.arguments, vec![1, 2]);
        assert_eq!(ins.line, 1);
    }
}
