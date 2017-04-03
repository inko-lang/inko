//! VM instruction handlers for constant operations.
use immix::copy_object::CopyObject;

use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets a constant in a given object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register pointing to the object to store the constant in.
/// 2. The register containing the constant name as a string.
/// 3. The register pointing to the object to store.
#[inline(always)]
pub fn set_const(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let target_ptr = process.get_register(instruction.arg(0));
    let name_ptr = process.get_register(instruction.arg(1));
    let source_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    if source_ptr.is_tagged_integer() {
        panic!("constants can not be added to integers");
    }

    let source = copy_if_permanent!(machine.state.permanent_allocator,
                                    source_ptr,
                                    target_ptr);

    target_ptr.add_constant(&process, name, source);
}

/// Looks up a constant and stores it in a register.
///
/// This instruction takes 3 arguments:
///
/// 1. The register to store the constant in.
/// 2. The register pointing to an object in which to look for the
///    constant.
/// 3. The register containing the name of the constant as a string.
///
/// If the constant does not exist the target register is set to nil instead.
#[inline(always)]
pub fn get_const(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let src = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    let object = src.lookup_constant(&machine.state, &name)
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, object);
}

/// Returns true if a constant exists, false otherwise.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the resulting boolean in.
/// 2. The register containing the source object to check.
/// 3. The register containing the constant name as a string.
#[inline(always)]
pub fn const_exists(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction) {
    let register = instruction.arg(0);
    let source = process.get_register(instruction.arg(1));
    let name_ptr = process.get_register(instruction.arg(2));
    let name = machine.state.intern_pointer(&name_ptr).unwrap();

    if source.lookup_constant(&machine.state, &name).is_some() {
        process.set_register(register, machine.state.true_object);
    } else {
        process.set_register(register, machine.state.false_object);
    }
}
