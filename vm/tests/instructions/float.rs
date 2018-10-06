use libinko::object_value;
use libinko::pool::Worker;
use libinko::vm::instruction::InstructionType;
use libinko::vm::test::*;

macro_rules! test_op {
    ($ins_type: ident, $test_func: ident, $expected: expr) => (
        #[test]
        fn $test_func() {
            let (machine, mut block, process) = setup();

            block.code.instructions =
                vec![new_instruction(InstructionType::$ins_type, vec![2, 0, 1]),
                     new_instruction(InstructionType::Return, vec![2])];

            let left = process
                .allocate_without_prototype(object_value::float(5.0));

            let right = process
                .allocate_without_prototype(object_value::float(2.0));

            process.set_register(0, left);
            process.set_register(1, right);

            machine.run(&Worker::new(0), &process).unwrap();

            let pointer = process.get_register(2);

            assert_eq!(pointer.float_value().unwrap(), $expected);
        }
    );
}

macro_rules! test_bool_op {
    ($ins_type: ident, $test_func: ident, $expected: ident) => (
        #[test]
        fn $test_func() {
            let (machine, mut block, process) = setup();
            block.code.instructions =
                vec![new_instruction(InstructionType::$ins_type, vec![2, 0, 1]),
                     new_instruction(InstructionType::Return, vec![2])];

            let left = process
                .allocate_without_prototype(object_value::float(5.0));

            let right = process
                .allocate_without_prototype(object_value::float(2.0));

            process.set_register(0, left);
            process.set_register(1, right);

            machine.run(&Worker::new(0), &process).unwrap();

            let pointer = process.get_register(2);

            assert!(pointer == machine.state.$expected);
        }
    );
}

#[test]
fn test_float_to_integer() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::FloatToInteger, vec![1, 0]),
            new_instruction(InstructionType::Return, vec![1]),
        ];

    let original = process.allocate_without_prototype(object_value::float(5.5));

    process.set_register(0, original);

    machine.run(&Worker::new(0), &process).unwrap();

    let pointer = process.get_register(1);

    assert!(pointer.integer_value().unwrap() == 5);
}

#[test]
fn test_float_to_string() {
    let (machine, mut block, process) = setup();

    block.code.instructions =
        vec![
            new_instruction(InstructionType::FloatToString, vec![1, 0]),
            new_instruction(InstructionType::Return, vec![1]),
        ];

    let original = process.allocate_without_prototype(object_value::float(5.5));

    process.set_register(0, original);

    machine.run(&Worker::new(0), &process).unwrap();

    let pointer = process.get_register(1);

    assert!(pointer.string_value().unwrap() == &"5.5".to_string());
}

test_op!(FloatAdd, test_float_add, 7.0);
test_op!(FloatDiv, test_float_div, 2.5);
test_op!(FloatMul, test_float_mul, 10.0);
test_op!(FloatSub, test_float_sub, 3.0);
test_op!(FloatMod, test_float_mod, 1.0);

test_bool_op!(FloatSmaller, test_float_smaller, false_object);
test_bool_op!(FloatGreater, test_float_greater, true_object);
test_bool_op!(FloatEquals, test_float_equals, false_object);
