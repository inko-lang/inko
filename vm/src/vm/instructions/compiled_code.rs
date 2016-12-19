//! VM instruction handlers for compiled code operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets a CompiledCode object in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the object in.
/// 2. The index of the compiled code object to store.
pub fn set_compiled_code(machine: &Machine,
                         process: &RcProcess,
                         code: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0)?;
    let cc_index = instruction.arg(1)?;

    let cc = code.code_object(cc_index)?;

    let obj = process.allocate(object_value::compiled_code(cc),
                               machine.state
                                   .compiled_code_prototype
                                   .clone());

    process.set_register(register, obj);

    Ok(Action::None)
}
