//! VM instruction handlers for array operations.
use immix::copy_object::CopyObject;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Returns a vector index for an i64
macro_rules! int_to_vector_index {
    ($vec: expr, $index: expr) => ({
        if $index >= 0 as i64 {
            $index as usize
        }
        else {
            ($vec.len() as i64 + $index) as usize
        }
    });
}

/// Sets an array in a register.
///
/// This instruction requires at least one argument: the register to store
/// the resulting array in. Any extra instruction arguments should point to
/// registers containing objects to store in the array.
#[inline(always)]
pub fn set_array(machine: &Machine,
                 process: &RcProcess,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let val_count = instruction.arguments.len() - 1;

    let values = machine.collect_arguments(&process, instruction, 1, val_count);

    let obj = process.allocate(object_value::array(values),
                               machine.state.array_prototype);

    process.set_register(register, obj);
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
/// If an index is out of bounds the array is filled with nil values. A negative
/// index can be used to indicate a position from the end of the array.
#[inline(always)]
pub fn array_insert(machine: &Machine,
                    process: &RcProcess,
                    instruction: &Instruction) {
    let register = instruction.arg(0);
    let array_ptr = process.get_register(instruction.arg(1));
    let index_ptr = process.get_register(instruction.arg(2));
    let value_ptr = process.get_register(instruction.arg(3));

    let mut vector = array_ptr.array_value_mut().unwrap();
    let index = int_to_vector_index!(vector, index_ptr.integer_value().unwrap());

    let value = copy_if_permanent!(machine.state.permanent_allocator,
                                   value_ptr,
                                   array_ptr);

    if index >= vector.len() {
        vector.resize(index + 1, machine.state.nil_object);
    }

    vector[index] = value;

    process.set_register(register, value);
}

/// Gets the value of an array index.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the value in.
/// 2. The register containing the array.
/// 3. The register containing the index.
///
/// This instruction will set nil in the target register if the array index is
/// out of bounds. A negative index can be used to indicate a position from the
/// end of the array.
#[inline(always)]
pub fn array_at(machine: &Machine,
                process: &RcProcess,
                instruction: &Instruction) {
    let register = instruction.arg(0);
    let array_ptr = process.get_register(instruction.arg(1));
    let index_ptr = process.get_register(instruction.arg(2));

    let vector = array_ptr.array_value().unwrap();
    let index = int_to_vector_index!(vector, index_ptr.integer_value().unwrap());

    let value = vector.get(index)
        .cloned()
        .unwrap_or_else(|| machine.state.nil_object);

    process.set_register(register, value);
}

/// Removes a value from an array.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the removed value in.
/// 2. The register containing the array to remove a value from.
/// 3. The register containing the index.
///
/// This instruction sets nil in the target register if the index is out of
/// bounds. A negative index can be used to indicate a position from the end of
/// the array.
#[inline(always)]
pub fn array_remove(machine: &Machine,
                    process: &RcProcess,
                    instruction: &Instruction) {
    let register = instruction.arg(0);
    let array_ptr = process.get_register(instruction.arg(1));
    let index_ptr = process.get_register(instruction.arg(2));

    let mut vector = array_ptr.array_value_mut().unwrap();
    let index = int_to_vector_index!(vector, index_ptr.integer_value().unwrap());

    let value = if index > vector.len() {
        machine.state.nil_object
    } else {
        vector.remove(index)
    };

    process.set_register(register, value);
}

/// Gets the amount of elements in an array.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the length in.
/// 2. The register containing the array.
#[inline(always)]
pub fn array_length(process: &RcProcess, instruction: &Instruction) {
    let register = instruction.arg(0);
    let array_ptr = process.get_register(instruction.arg(1));
    let vector = array_ptr.array_value().unwrap();
    let length = vector.len() as i64;

    process.set_register(register, ObjectPointer::integer(length));
}

/// Removes all elements from an array.
///
/// This instruction requires 1 argument: the register of the array.
#[inline(always)]
pub fn array_clear(process: &RcProcess, instruction: &Instruction) {
    let array_ptr = process.get_register(instruction.arg(0));
    let mut vector = array_ptr.array_value_mut().unwrap();

    vector.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_value;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    #[test]
    fn test_set_array() {
        let (machine, _block, process) = setup();

        let instruction = new_instruction(InstructionType::SetArray,
                                          vec![2, 0, 1]);

        let value1 = process.allocate_empty();
        let value2 = process.allocate_empty();

        process.set_register(0, value1);
        process.set_register(1, value2);

        set_array(&machine, &process, &instruction);

        let pointer = process.get_register(2);
        let object = pointer.get();

        assert!(object.value.is_array());

        let values = object.value.as_array().unwrap();

        assert_eq!(values.len(), 2);

        assert!(values[0] == value1);
        assert!(values[1] == value2);
    }

    #[test]
    fn test_array_insert() {
        let (machine, _block, process) = setup();
        let instruction = new_instruction(InstructionType::ArrayInsert,
                                          vec![3, 0, 1, 2]);

        let array =
            process.allocate_without_prototype(object_value::array(Vec::new()));

        let index = ObjectPointer::integer(0);
        let value = ObjectPointer::integer(5);

        process.set_register(0, array);
        process.set_register(1, index);
        process.set_register(2, value);

        array_insert(&machine, &process, &instruction);

        let pointer = process.get_register(3);

        assert_eq!(pointer.integer_value().unwrap(), 5);
    }

    #[test]
    fn test_array_at() {
        let (machine, _block, process) = setup();
        let instruction = new_instruction(InstructionType::ArrayAt,
                                          vec![2, 0, 1]);

        let value = ObjectPointer::integer(5);

        let array =
            process.allocate_without_prototype(object_value::array(vec![value]));

        let index = ObjectPointer::integer(0);

        process.set_register(0, array);
        process.set_register(1, index);

        array_at(&machine, &process, &instruction);

        let pointer = process.get_register(2);

        assert_eq!(pointer.integer_value().unwrap(), 5);
    }

    #[test]
    fn test_array_remove() {
        let (machine, _block, process) = setup();
        let instruction = new_instruction(InstructionType::ArrayRemove,
                                          vec![2, 0, 1]);

        let value = ObjectPointer::integer(5);

        let array =
            process.allocate_without_prototype(object_value::array(vec![value]));

        let index = ObjectPointer::integer(0);

        process.set_register(0, array);
        process.set_register(1, index);

        array_remove(&machine, &process, &instruction);

        let removed_pointer = process.get_register(2);

        assert_eq!(removed_pointer.integer_value().unwrap(), 5);

        assert_eq!(array.get()
                       .value
                       .as_array()
                       .unwrap()
                       .len(),
                   0);
    }

    #[test]
    fn test_array_length() {
        let (_machine, _block, process) = setup();
        let instruction = new_instruction(InstructionType::ArrayLength,
                                          vec![1, 0]);

        let value = process.allocate_empty();

        let array =
            process.allocate_without_prototype(object_value::array(vec![value]));

        process.set_register(0, array);

        array_length(&process, &instruction);

        let pointer = process.get_register(1);

        assert_eq!(pointer.integer_value().unwrap(), 1);
    }

    #[test]
    fn test_array_clear() {
        let (_machine, _block, process) = setup();
        let instruction = new_instruction(InstructionType::ArrayClear, vec![0]);

        let value = process.allocate_empty();

        let array =
            process.allocate_without_prototype(object_value::array(vec![value]));

        process.set_register(0, array);

        array_clear(&process, &instruction);

        let object = array.get();

        assert_eq!(object.value.as_array().unwrap().len(), 0);
    }
}
