//! VM instruction handlers for float operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets a float in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the float in.
/// 2. The index of the float literals to use for the value.
///
/// The float literal is extracted from the given CompiledCode.
pub fn set_float(machine: &Machine,
                 process: &RcProcess,
                 code: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let index = instruction.arg(1)?;
    let value = *code.float(index)?;

    let obj = process.allocate(object_value::float(value),
                               machine.state.float_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Adds two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to add.
pub fn float_add(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, +)
}

/// Multiplies two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to multiply with.
pub fn float_mul(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, *)
}

/// Divides two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to divide with.
pub fn float_div(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, /)
}

/// Subtracts two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to subtract.
pub fn float_sub(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, -)
}

/// Gets the modulo of a float
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float argument.
pub fn float_mod(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, %)
}

/// Converts a float to an integer
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
pub fn float_to_integer(machine: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let register = instruction.arg(0)?;
    let float_ptr = process.get_register(instruction.arg(1)?)?;
    let float = float_ptr.get();

    ensure_floats!(instruction, float);

    let result = float.value.as_float() as i64;

    let obj = process.allocate(object_value::integer(result),
                               machine.state.integer_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Converts a float to a string
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
pub fn float_to_string(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let float_ptr = process.get_register(instruction.arg(1)?)?;
    let float = float_ptr.get();

    ensure_floats!(instruction, float);

    let result = float.value.as_float().to_string();

    let obj = process.allocate(object_value::string(result),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Checks if one float is smaller than the other.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the float to compare.
/// 3. The register containing the float to compare with.
///
/// The result of this instruction is either boolean true or false.
pub fn float_smaller(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    float_bool_op!(machine, process, instruction, <)
}

/// Checks if one float is greater than the other.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the float to compare.
/// 3. The register containing the float to compare with.
///
/// The result of this instruction is either boolean true or false.
pub fn float_greater(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    float_bool_op!(machine, process, instruction, >)
}

/// Checks if two floats are equal.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the float to compare.
/// 3. The register containing the float to compare with.
///
/// The result of this instruction is either boolean true or false.
pub fn float_equals(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    float_bool_op!(machine, process, instruction, ==)
}
