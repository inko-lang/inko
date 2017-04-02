//! Functions for testing instruction handlers.
use std::sync::Arc;
use vm::instruction::{Instruction, InstructionType};
use vm::machine::Machine;
use vm::state::State;

use compiled_code::{RcCompiledCode, CompiledCode};
use config::Config;
use process::RcProcess;

/// Sets up a VM with a single process.
#[inline(always)]
pub fn setup() -> (Machine, RcCompiledCode, RcProcess) {
    let state = State::new(Config::new());
    let machine = Machine::default(state);

    let code =
        CompiledCode::with_rc("a".to_string(), "a".to_string(), 1, Vec::new());

    let process = machine.allocate_process(0, code.clone());

    (machine, code, process.unwrap())
}

/// Creates a new instruction.
#[inline(always)]
pub fn new_instruction(ins_type: InstructionType, args: Vec<u16>) -> Instruction {
    Instruction::new(ins_type, args, 1)
}

/// Returns a mutable reference to the wrapped value of an Arc, regardless of
/// the number of references.
///
/// Callers should ensure the wrapped value is not modified concurrently.
#[inline(always)]
pub fn arc_mut<T>(arc: &Arc<T>) -> &mut T {
    let ptr = &**arc as *const T as *mut T;

    unsafe { &mut *ptr }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vm::instruction::InstructionType;

    #[test]
    fn test_new_instruction() {
        let ins = new_instruction(InstructionType::SetInteger, vec![1, 2]);

        assert_eq!(ins.instruction_type, InstructionType::SetInteger);
        assert_eq!(ins.arguments, vec![1, 2]);
        assert_eq!(ins.line, 1);
    }

    #[test]
    fn test_arc_mut() {
        let value = Arc::new(10);

        *arc_mut(&value) = 15;

        assert_eq!(*value, 15);
    }
}
