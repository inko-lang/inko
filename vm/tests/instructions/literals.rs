use libinko::object_pointer::ObjectPointer;
use libinko::vm::instruction::InstructionType;
use libinko::vm::test::*;

#[test]
fn test_set_literal() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![new_instruction(InstructionType::SetLiteral, vec![0, 0]),
             new_instruction(InstructionType::Return, vec![0])];

    block.code.literals.push(ObjectPointer::integer(10));

    machine.run(&process);

    let pointer = process.get_register(0);

    assert_eq!(pointer.integer_value().unwrap(), 10);
}
