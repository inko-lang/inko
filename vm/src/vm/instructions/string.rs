//! VM instruction handlers for string operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use errors;
use object_value;
use process::RcProcess;

/// Sets a string in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the float in.
/// 2. The index of the string literal to use for the value.
///
/// The string literal is extracted from the given CompiledCode.
pub fn set_string(machine: &Machine,
                  process: &RcProcess,
                  code: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let index = instruction.arg(1)?;
    let value = code.string(index)?;

    let obj = process.allocate(object_value::string(value.clone()),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Returns the lowercase equivalent of a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the new string in.
/// 2. The register containing the input string.
pub fn string_to_lower(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let source_ptr = process.get_register(instruction.arg(1)?)?;
    let source = source_ptr.get();
    let lower = source.value.as_string()?.to_lowercase();

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
pub fn string_to_upper(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let source_ptr = process.get_register(instruction.arg(1)?)?;
    let source = source_ptr.get();
    let upper = source.value.as_string()?.to_uppercase();

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
pub fn string_equals(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0)?;
    let receiver_ptr = process.get_register(instruction.arg(1)?)?;
    let arg_ptr = process.get_register(instruction.arg(2)?)?;

    let receiver = receiver_ptr.get();
    let arg = arg_ptr.get();
    let result = receiver.value.as_string()? == arg.value.as_string()?;

    let boolean = if result {
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
pub fn string_to_bytes(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let arg_ptr = process.get_register(instruction.arg(1)?)?;

    let arg = arg_ptr.get();

    ensure_strings!(instruction, arg);

    let int_proto = machine.state.integer_prototype.clone();
    let array_proto = machine.state.array_prototype.clone();

    let array = arg.value
        .as_string()?
        .as_bytes()
        .iter()
        .map(|&b| {
            process.allocate(object_value::integer(b as i64), int_proto.clone())
        })
        .collect::<Vec<_>>();

    let obj = process.allocate(object_value::array(array), array_proto);

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
pub fn string_from_bytes(machine: &Machine,
                         process: &RcProcess,
                         _: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0)?;
    let arg_ptr = process.get_register(instruction.arg(1)?)?;

    let arg = arg_ptr.get();
    let array = arg.value.as_array()?;
    let mut bytes = Vec::with_capacity(array.len());

    for ptr in array.iter() {
        let integer = ptr.get().value.as_integer()?;

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
pub fn string_length(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    let register = instruction.arg(0)?;
    let arg_ptr = process.get_register(instruction.arg(1)?)?;

    let arg = arg_ptr.get();
    let int_proto = machine.state.integer_prototype.clone();
    let length = arg.value.as_string()?.chars().count() as i64;

    let obj = process.allocate(object_value::integer(length), int_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Returns the amount of bytes in a string.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the string.
pub fn string_size(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let arg_ptr = process.get_register(instruction.arg(1)?)?;

    let arg = arg_ptr.get();
    let int_proto = machine.state.integer_prototype.clone();
    let size = arg.value.as_string()?.len() as i64;

    let obj = process.allocate(object_value::integer(size), int_proto);

    process.set_register(register, obj);

    Ok(Action::None)
}
