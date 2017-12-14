use libinko::object_pointer::ObjectPointer;
use libinko::object_value;
use libinko::vm::instruction::InstructionType;
use libinko::vm::test::*;

#[test]
fn test_set_array() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::SetArray, vec![2, 0, 1]),
            new_instruction(InstructionType::Return, vec![2]),
        ];

    let value1 = process.allocate_empty();
    let value2 = process.allocate_empty();

    process.set_register(0, value1);
    process.set_register(1, value2);

    machine.run(&process);

    let pointer = process.get_register(2);
    let object = pointer.get();

    assert!(object.value.is_array());

    let values = object.value.as_array().unwrap();

    assert_eq!(values.len(), 2);

    assert!(values[0] == value1);
    assert!(values[1] == value2);
}

#[test]
fn test_array_set() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::ArraySet, vec![3, 0, 1, 2]),
            new_instruction(InstructionType::Return, vec![3]),
        ];

    let array =
        process.allocate_without_prototype(object_value::array(Vec::new()));

    let index = ObjectPointer::integer(0);
    let value = ObjectPointer::integer(5);

    process.set_register(0, array);
    process.set_register(1, index);
    process.set_register(2, value);

    machine.run(&process);

    let pointer = process.get_register(3);

    assert_eq!(pointer.integer_value().unwrap(), 5);
}

#[test]
fn test_array_at() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::ArrayAt, vec![2, 0, 1]),
            new_instruction(InstructionType::Return, vec![2]),
        ];

    let value = ObjectPointer::integer(5);

    let array =
        process.allocate_without_prototype(object_value::array(vec![value]));

    let index = ObjectPointer::integer(0);

    process.set_register(0, array);
    process.set_register(1, index);

    machine.run(&process);

    let pointer = process.get_register(2);

    assert_eq!(pointer.integer_value().unwrap(), 5);
}

#[test]
fn test_array_remove() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::ArrayRemove, vec![2, 0, 1]),
            new_instruction(InstructionType::Return, vec![2]),
        ];

    let value = ObjectPointer::integer(5);

    let array =
        process.allocate_without_prototype(object_value::array(vec![value]));

    let index = ObjectPointer::integer(0);

    process.set_register(0, array);
    process.set_register(1, index);

    machine.run(&process);

    let removed_pointer = process.get_register(2);

    assert_eq!(removed_pointer.integer_value().unwrap(), 5);

    assert_eq!(array.get().value.as_array().unwrap().len(), 0);
}

#[test]
fn test_array_length() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::ArrayLength, vec![1, 0]),
            new_instruction(InstructionType::Return, vec![1]),
        ];

    let value = process.allocate_empty();
    let array =
        process.allocate_without_prototype(object_value::array(vec![value]));

    process.set_register(0, array);

    machine.run(&process);

    let pointer = process.get_register(1);

    assert_eq!(pointer.integer_value().unwrap(), 1);
}

#[test]
fn test_array_clear() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::ArrayClear, vec![0]),
            new_instruction(InstructionType::Return, vec![0]),
        ];

    let value = process.allocate_empty();

    let array =
        process.allocate_without_prototype(object_value::array(vec![value]));

    process.set_register(0, array);

    machine.run(&process);

    let object = array.get();

    assert_eq!(object.value.as_array().unwrap().len(), 0);
}
