//! VM instruction handlers for getting/setting object prototypes.
use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Sets the prototype of an object.
///
/// This instruction requires two arguments:
///
/// 1. The register containing the object for which to set the prototype.
/// 2. The register containing the object to use as the prototype.
#[inline(always)]
pub fn set_prototype(_: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction) {
    let source = process.get_register(instruction.arg(0));
    let proto = process.get_register(instruction.arg(1));

    source.get_mut().set_prototype(proto);
}

/// Gets the prototype of an object.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the prototype in.
/// 2. The register containing the object to get the prototype from.
///
/// If no prototype was found, nil is set in the register instead.
#[inline(always)]
pub fn get_prototype(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction) {
    let register = instruction.arg(0);
    let source = process.get_register(instruction.arg(1));

    let proto = source.prototype(&machine.state)
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, proto);
}

/// Returns the prototype to use for integer objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_integer_prototype(machine: &Machine,
                             process: &RcProcess,
                             _: &RcCompiledCode,
                             instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.integer_prototype);
}

/// Returns the prototype to use for float objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_float_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.float_prototype);
}

/// Returns the prototype to use for string objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_string_prototype(machine: &Machine,
                            process: &RcProcess,
                            _: &RcCompiledCode,
                            instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.string_prototype);
}

/// Returns the prototype to use for array objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_array_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.array_prototype);
}

/// Gets the prototype to use for true objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_true_prototype(machine: &Machine,
                          process: &RcProcess,
                          _: &RcCompiledCode,
                          instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.true_prototype);
}

/// Gets the prototype to use for false objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_false_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.false_prototype);
}

/// Gets the prototype to use for method objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_method_prototype(machine: &Machine,
                            process: &RcProcess,
                            _: &RcCompiledCode,
                            instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.method_prototype);
}

/// Gets the prototype to use for Binding objects.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_binding_prototype(machine: &Machine,
                             process: &RcProcess,
                             _: &RcCompiledCode,
                             instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.binding_prototype);
}

/// Gets the prototype to use for Blocks.
///
/// This instruction requires one argument: the register to store the
/// prototype in.
#[inline(always)]
pub fn get_block_prototype(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.block_prototype);
}

/// Gets the prototype to use for "nil" objects.
///
/// This instruction requires one argument: the register to store the prototype
/// in.
#[inline(always)]
pub fn get_nil_prototype(machine: &Machine,
                         process: &RcProcess,
                         _: &RcCompiledCode,
                         instruction: &Instruction) {
    let register = instruction.arg(0);

    process.set_register(register, machine.state.nil_prototype);
}
