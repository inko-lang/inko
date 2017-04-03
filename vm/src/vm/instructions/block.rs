//! VM instruction handlers for Block operations.
use vm::instruction::Instruction;
use vm::machine::Machine;

use block::Block;
use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets a Block in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the object in.
/// 2. The index of the CompiledCode object literal to use for creating the
///    Block.
#[inline(always)]
pub fn set_block(machine: &Machine,
                 process: &RcProcess,
                 code: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let cc_index = instruction.arg(1);

    let cc = code.code_object(cc_index);
    let binding = process.binding();
    let block = Block::new(cc.clone(), binding);

    let obj = process.allocate(object_value::block(block),
                               machine.state.block_prototype);

    process.set_register(register, obj);
}
