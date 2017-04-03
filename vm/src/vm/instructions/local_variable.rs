//! VM instruction handlers for local variable operations.
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Sets a local variable to a given register's value.
///
/// This instruction requires two arguments:
///
/// 1. The local variable index to set.
/// 2. The register containing the object to store in the variable.
#[inline(always)]
pub fn set_local(process: &RcProcess, instruction: &Instruction) {
    let local_index = instruction.arg(0);
    let object = process.get_register(instruction.arg(1));

    process.set_local(local_index, object);
}

/// Gets a local variable and stores it in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the local's value in.
/// 2. The local variable index to get the value from.
#[inline(always)]
pub fn get_local(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let local_index = instruction.arg(1);
    let object = process.get_local(local_index);

    process.set_register(register, object);
}

/// Checks if a local variable exists.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in (true or false).
/// 2. The local variable index to check.
#[inline(always)]
pub fn local_exists(machine: &Machine,
                    process: &RcProcess,
                    instruction: &Instruction) {
    let register = instruction.arg(0);
    let local_index = instruction.arg(1);

    let value = if process.local_exists(local_index) {
        machine.state.true_object
    } else {
        machine.state.false_object
    };

    process.set_register(register, value);
}

/// Sets a local variable in one of the parent bindings.
///
/// This instruction requires 3 arguments:
///
/// 1. The local variable index to set.
/// 2. The number of parent bindings to traverse in order to find the
///    binding to set the variable in.
/// 3. The register containing the value to set.
#[inline(always)]
pub fn set_parent_local(process: &RcProcess, instruction: &Instruction) {
    let index = instruction.arg(0);
    let depth = instruction.arg(1);
    let value = process.get_register(instruction.arg(2));

    if let Some(binding) = process.binding().find_parent(depth) {
        binding.set_local(index, value);
    } else {
        panic!("No binding for depth {}", depth);
    }
}

/// Gets a local variable in one of the parent bindings.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the local variable in.
/// 2. The number of parent bindings to traverse in order to find the
///    binding to get the variable from.
/// 3. The local variable index to get.
#[inline(always)]
pub fn get_parent_local(process: &RcProcess, instruction: &Instruction) {
    let reg = instruction.arg(0);
    let depth = instruction.arg(1);
    let index = instruction.arg(2);

    if let Some(binding) = process.binding().find_parent(depth) {
        process.set_register(reg, binding.get_local(index));
    } else {
        panic!("No binding for depth {}", depth);
    }
}
