//! VM instruction handlers for array operations.
use immix::copy_object::CopyObject;

use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets an array in a register.
///
/// This instruction requires at least one argument: the register to store
/// the resulting array in. Any extra instruction arguments should point to
/// registers containing objects to store in the array.
pub fn set_array(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let val_count = instruction.arguments.len() - 1;

    let values =
        machine.collect_arguments(process.clone(), instruction, 1, val_count)?;

    let obj = process.allocate(object_value::array(values),
                               machine.state.array_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Inserts a value in an array.
///
/// This instruction requires 4 arguments:
///
/// 1. The register to store the result (the inserted value) in.
/// 2. The register containing the array to insert into.
/// 3. The register containing the index (as an integer) to insert at.
/// 4. The register containing the value to insert.
///
/// An error is returned when the index is greater than the array length. A
/// negative index can be used to indicate a position from the end of the
/// array.
pub fn array_insert(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let register = instruction.arg(0)?;
    let array_ptr = process.get_register(instruction.arg(1)?)?;
    let index_ptr = process.get_register(instruction.arg(2)?)?;
    let value_ptr = process.get_register(instruction.arg(3)?)?;

    let mut array = array_ptr.get_mut();
    let index_obj = index_ptr.get();

    let mut vector = array.value.as_array_mut()?;
    let index = int_to_vector_index!(vector, index_obj.value.as_integer()?);

    ensure_array_within_bounds!(instruction, vector, index);

    let value = copy_if_permanent!(machine.state.permanent_allocator,
                                   value_ptr,
                                   array_ptr);

    if vector.get(index).is_some() {
        vector[index] = value;
    } else {
        vector.insert(index, value);
    }

    process.set_register(register, value);

    Ok(Action::None)
}

/// Gets the value of an array index.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the value in.
/// 2. The register containing the array.
/// 3. The register containing the index.
///
/// An error is returned when the index is greater than the array length. A
/// negative index can be used to indicate a position from the end of the
/// array.
pub fn array_at(_: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    let register = instruction.arg(0)?;
    let array_ptr = process.get_register(instruction.arg(1)?)?;
    let index_ptr = process.get_register(instruction.arg(2)?)?;
    let array = array_ptr.get();

    let index_obj = index_ptr.get();
    let vector = array.value.as_array()?;
    let index = int_to_vector_index!(vector, index_obj.value.as_integer()?);

    ensure_array_within_bounds!(instruction, vector, index);

    let value = vector[index].clone();

    process.set_register(register, value);

    Ok(Action::None)
}

/// Removes a value from an array.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the removed value in.
/// 2. The register containing the array to remove a value from.
/// 3. The register containing the index.
///
/// An error is returned when the index is greater than the array length. A
/// negative index can be used to indicate a position from the end of the
/// array.
pub fn array_remove(_: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let register = instruction.arg(0)?;
    let array_ptr = process.get_register(instruction.arg(1)?)?;
    let index_ptr = process.get_register(instruction.arg(2)?)?;

    let mut array = array_ptr.get_mut();
    let index_obj = index_ptr.get();
    let mut vector = array.value.as_array_mut()?;
    let index = int_to_vector_index!(vector, index_obj.value.as_integer()?);

    ensure_array_within_bounds!(instruction, vector, index);

    let value = vector.remove(index);

    process.set_register(register, value);

    Ok(Action::None)
}

/// Gets the amount of elements in an array.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the length in.
/// 2. The register containing the array.
pub fn array_length(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let register = instruction.arg(0)?;
    let array_ptr = process.get_register(instruction.arg(1)?)?;
    let array = array_ptr.get();
    let vector = array.value.as_array()?;
    let length = vector.len() as i64;

    let obj = process.allocate(object_value::integer(length),
                               machine.state.integer_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Removes all elements from an array.
///
/// This instruction requires 1 argument: the register of the array.
pub fn array_clear(_: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let array_ptr = process.get_register(instruction.arg(0)?)?;
    let mut array = array_ptr.get_mut();
    let mut vector = array.value.as_array_mut()?;

    vector.clear();

    Ok(Action::None)
}
