//! VM instruction handlers for binding operations.
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Gets the Binding of the current scope and sets it in a register
///
/// This instruction requires only one argument: the register to store the
/// object in.
#[inline(always)]
pub fn get_binding(machine: &Machine,
                   process: &RcProcess,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let binding = process.binding();

    let obj = process.allocate(object_value::binding(binding),
                               machine.state.binding_prototype);

    process.set_register(register, obj);
}
