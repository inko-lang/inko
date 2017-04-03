//! VM instruction handlers for nil objects.
use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a "nil" object in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_nil(machine: &Machine,
               process: &RcProcess,
               _: &RcCompiledCode,
               instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.nil_object);
}
