//! VM instruction handlers for string operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use errors;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;

/// Sets a string literal in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the float in.
/// 2. The index of the string literal to use for the value.
///
/// String literals are interned to prevent allocating objects for identical
/// strings.
#[inline(always)]
pub fn set_string(_: &Machine,
                  process: &RcProcess,
                  code: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0);
    let index = instruction.arg(1);

    process.set_register(register, code.string(index));

    Ok(Action::None)
}

/// Returns the lowercase equivalent of a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the new string in.
/// 2. The register containing the input string.
#[inline(always)]
pub fn string_to_lower(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0);
    let source_ptr = process.get_register(instruction.arg(1));
    let lower = source_ptr.string_value()?.to_lowercase();

    let obj = process.allocate(object_value::string(lower),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Returns the uppercase equivalent of a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the new string in.
/// 2. The register containing the input string.
#[inline(always)]
pub fn string_to_upper(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0);
    let source_ptr = process.get_register(instruction.arg(1));
    let upper = source_ptr.string_value()?.to_uppercase();

    let obj = process.allocate(object_value::string(upper),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Checks if two strings are equal.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the string to compare.
/// 3. The register of the string to compare with.
#[inline(always)]
pub fn string_equals(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0);
    let receiver_ptr = process.get_register(instruction.arg(1));
    let arg_ptr = process.get_register(instruction.arg(2));

    let boolean = if receiver_ptr.string_value()? == arg_ptr.string_value()? {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, boolean);

    Ok(Action::None)
}

/// Returns an array containing the bytes of a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the string to get the bytes from.
#[inline(always)]
pub fn string_to_bytes(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0);
    let string_ptr = process.get_register(instruction.arg(1));

    let array = string_ptr.string_value()?
        .as_bytes()
        .iter()
        .map(|&b| ObjectPointer::integer(b as i64))
        .collect::<Vec<_>>();

    let obj = process.allocate(object_value::array(array),
                               machine.state.array_prototype);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Creates a string from an array of bytes
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the array of bytes.
///
/// The result of this instruction is either a string based on the given
/// bytes, or an error object.
#[inline(always)]
pub fn string_from_bytes(machine: &Machine,
                         process: &RcProcess,
                         _: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0);
    let arg_ptr = process.get_register(instruction.arg(1));

    let array = arg_ptr.array_value()?;
    let mut bytes = Vec::with_capacity(array.len());

    for ptr in array.iter() {
        let integer = ptr.integer_value()?;

        bytes.push(integer as u8);
    }

    let obj = match String::from_utf8(bytes) {
        Ok(string) => {
            process.allocate(object_value::string(string),
                             machine.state.string_prototype)
        }
        Err(_) => {
            let code = errors::string::invalid_utf8();

            process.allocate_without_prototype(object_value::error(code))
        }
    };

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Returns the amount of characters in a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the string.
#[inline(always)]
pub fn string_length(_: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0);
    let arg_ptr = process.get_register(instruction.arg(1));

    let length = arg_ptr.string_value()?.chars().count() as i64;

    process.set_register(register, ObjectPointer::integer(length));

    Ok(Action::None)
}

/// Returns the amount of bytes in a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the string.
#[inline(always)]
pub fn string_size(_: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0);
    let arg_ptr = process.get_register(instruction.arg(1));

    let size = arg_ptr.string_value()?.len() as i64;

    process.set_register(register, ObjectPointer::integer(size));

    Ok(Action::None)
}
