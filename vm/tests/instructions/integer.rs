use libinko::object_pointer::ObjectPointer;
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

            let left = ObjectPointer::integer(5);
            let right = ObjectPointer::integer(2);

            process.set_register(0, left);
            process.set_register(1, right);

            machine.run(&process).unwrap();

            let pointer = process.get_register(2);

            assert_eq!(pointer.integer_value().unwrap(), $expected);
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

            let left = ObjectPointer::integer(5);
            let right = ObjectPointer::integer(2);

            process.set_register(0, left);
            process.set_register(1, right);

            machine.run(&process).unwrap();

            let pointer = process.get_register(2);

            assert!(pointer == machine.state.$expected);
        }
    );
}

macro_rules! test_cast_op {
    ($ins_type: ident, $test_func:ident, $target_type: ident, $target_val: expr) => (
        #[test]
        fn $test_func() {
            let (machine, mut block, process) = setup();

            block.code.instructions =
                vec![new_instruction(InstructionType::$ins_type, vec![1, 0]),
                     new_instruction(InstructionType::Return, vec![1])];

            let original = ObjectPointer::integer(5);

            process.set_register(0, original);

            machine.run(&process).unwrap();

            let pointer = process.get_register(1);
            let object = pointer.get();

            assert!(object.value.$target_type().unwrap() == $target_val);
        }
    );
}

test_op!(IntegerAdd, test_integer_add, 7);
test_op!(IntegerDiv, test_integer_div, 2);
test_op!(IntegerMul, test_integer_mul, 10);
test_op!(IntegerSub, test_integer_sub, 3);
test_op!(IntegerMod, test_integer_mod, 1);

test_op!(IntegerBitwiseAnd, test_integer_bitwise_and, 0);

test_op!(IntegerBitwiseOr, test_integer_bitwise_or, 7);

test_op!(IntegerBitwiseXor, test_integer_bitwise_xor, 7);

test_op!(IntegerShiftLeft, test_integer_shift_left, 20);

test_op!(IntegerShiftRight, test_integer_shift_right, 1);

test_bool_op!(IntegerSmaller, test_integer_smaller, false_object);

test_bool_op!(IntegerGreater, test_integer_greater, true_object);

test_bool_op!(IntegerEquals, test_integer_equals, false_object);

test_cast_op!(IntegerToFloat, test_integer_to_float, as_float, 5.0);

test_cast_op!(
    IntegerToString,
    test_integer_to_string,
    as_string,
    &"5".to_string()
);
