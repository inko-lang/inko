//! Functions for testing instruction handlers.
use crate::block::Block;
use crate::compiled_code::CompiledCode;
use crate::config::Config;
use crate::module::Module;
use crate::process::RcProcess;
use crate::vm::instruction::{Instruction, Opcode};
use crate::vm::instructions::process;
use crate::vm::machine::Machine;
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
    let machine = Machine::new(state);
    let mut code = CompiledCode::new(
        name,
        name,
        1,
        vec![Instruction::new(Opcode::Return, [0, 0, 0, 0, 0, 0], 1)],
    );

    // Reserve enough space for registers/locals for most tests.
    code.locals = 32;
    code.registers = 1024;

    let (block, process) = {
        let mut modules = machine.state.modules.lock();
        let mod_name = machine.state.intern_string("test".to_string());
        let mod_path = machine.state.intern_string(
            if cfg!(windows) { "C:\\test" } else { "/test" }.to_string(),
        );

        modules.add(
            &machine.state,
            vec![Module::new(mod_name, mod_path, code, Vec::new())],
        );

        let module_ptr = modules.get(&"test").unwrap();
        let module = module_ptr.module_value().unwrap();
        let block = Block::new(module.code(), None, module_ptr, module);
        let process = process::process_allocate(&machine.state, &block);

        process.set_main();

        (block, process)
    };

    (machine, block, process)
}
