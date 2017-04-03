//! VM instruction handlers for nil objects.
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Sets a "nil" object in a register.
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_nil(machine: &Machine,
               process: &RcProcess,
               instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.nil_object);
}
