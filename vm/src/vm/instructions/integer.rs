//! VM instruction handlers for integer operations.
use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::CompiledCodePointer;
use object_value;
use object_pointer::ObjectPointer;
use process::RcProcess;

/// Sets an integer in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the integer in.
/// 2. The index of the integer literals to use for the value.
#[inline(always)]
pub fn set_integer(process: &RcProcess,
                   code: &CompiledCodePointer,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let index = instruction.arg(1);

    process.set_register(register, code.integer(index));
}

/// Adds two integers
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
#[inline(always)]
pub fn integer_add(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, +);
}

/// Divides an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
#[inline(always)]
pub fn integer_div(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, /);
}

/// Multiplies an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
#[inline(always)]
pub fn integer_mul(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, *);
}

/// Subtracts an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
#[inline(always)]
pub fn integer_sub(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, -);
}

/// Gets the modulo of an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
#[inline(always)]
pub fn integer_mod(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, %);
}

/// Converts an integer to a float
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to convert.
#[inline(always)]
pub fn integer_to_float(machine: &Machine,
                        process: &RcProcess,
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let integer_ptr = process.get_register(instruction.arg(1));
    let result = integer_ptr.integer_value().unwrap() as f64;

    let obj = process.allocate(object_value::float(result),
                               machine.state.float_prototype);

    process.set_register(register, obj);
}

/// Converts an integer to a string
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to convert.
#[inline(always)]
pub fn integer_to_string(machine: &Machine,
                         process: &RcProcess,
                         instruction: &Instruction) {
    let register = instruction.arg(0);
    let integer_ptr = process.get_register(instruction.arg(1));
    let result = integer_ptr.integer_value().unwrap().to_string();

    let obj =
        process.allocate(object_value::string(result),
                         machine.state.string_prototype);

    process.set_register(register, obj);
}

/// Performs an integer bitwise AND.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
#[inline(always)]
pub fn integer_bitwise_and(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, &);
}

/// Performs an integer bitwise OR.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
#[inline(always)]
pub fn integer_bitwise_or(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, |);
}

/// Performs an integer bitwise XOR.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
#[inline(always)]
pub fn integer_bitwise_xor(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, ^);
}

/// Shifts an integer to the left.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
#[inline(always)]
pub fn integer_shift_left(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, <<);
}

/// Shifts an integer to the right.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
#[inline(always)]
pub fn integer_shift_right(process: &RcProcess, instruction: &Instruction) {
    integer_op!(process, instruction, >>);
}

/// Checks if one integer is smaller than the other.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the integer to compare.
/// 3. The register containing the integer to compare with.
///
/// The result of this instruction is either boolean true or false.
#[inline(always)]
pub fn integer_smaller(machine: &Machine,
                       process: &RcProcess,
                       instruction: &Instruction) {
    integer_bool_op!(machine, process, instruction, <);
}

/// Checks if one integer is greater than the other.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the integer to compare.
/// 3. The register containing the integer to compare with.
///
/// The result of this instruction is either boolean true or false.
#[inline(always)]
pub fn integer_greater(machine: &Machine,
                       process: &RcProcess,
                       instruction: &Instruction) {
    integer_bool_op!(machine, process, instruction, >);
}

/// Checks if two integers are equal.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register containing the integer to compare.
/// 3. The register containing the integer to compare with.
///
/// The result of this instruction is either boolean true or false.
#[inline(always)]
pub fn integer_equals(machine: &Machine,
                      process: &RcProcess,
                      instruction: &Instruction) {
    integer_bool_op!(machine, process, instruction, ==);
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    macro_rules! test_op {
        ($ins_type: ident, $test_func: ident, $ins_func: ident, $expected: expr) => (
            #[test]
            fn $test_func() {
                let (_machine, _block, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![2, 0, 1]);

                let left = ObjectPointer::integer(5);
                let right = ObjectPointer::integer(2);

                process.set_register(0, left);
                process.set_register(1, right);

                $ins_func(&process, &instruction);

                let pointer = process.get_register(2);

                assert_eq!(pointer.integer_value().unwrap(), $expected);
            }
        );
    }

    macro_rules! test_bool_op {
        ($ins_type: ident, $test_func: ident, $ins_func: ident, $expected: ident) => (
            #[test]
            fn $test_func() {
                let (machine, _block, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![2, 0, 1]);

                let left = ObjectPointer::integer(5);
                let right = ObjectPointer::integer(2);

                process.set_register(0, left);
                process.set_register(1, right);

                $ins_func(&machine, &process, &instruction);

                let pointer = process.get_register(2);

                assert!(pointer == machine.state.$expected);
            }
        );
    }

    macro_rules! test_cast_op {
        ($ins_type: ident, $test_func:ident, $ins_func: ident, $target_type: ident, $target_val: expr) => (
            #[test]
            fn $test_func() {
                let (machine, _block, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![1, 0]);

                let original = ObjectPointer::integer(5);

                process.set_register(0, original);

                $ins_func(&machine, &process, &instruction);

                let pointer = process.get_register(1);
                let object = pointer.get();

                assert!(object.value.$target_type().unwrap() == $target_val);
            }
        );
    }

    #[test]
    fn test_set_integer() {
        let (_machine, mut block, process) = setup();
        let instruction = new_instruction(InstructionType::SetInteger,
                                          vec![0, 0]);

        block.code.integer_literals.push(ObjectPointer::integer(10));

        set_integer(&process, &block.code, &instruction);

        let pointer = process.get_register(0);

        assert_eq!(pointer.integer_value().unwrap(), 10);
    }

    test_op!(IntegerAdd, test_integer_add, integer_add, 7);
    test_op!(IntegerDiv, test_integer_div, integer_div, 2);
    test_op!(IntegerMul, test_integer_mul, integer_mul, 10);
    test_op!(IntegerSub, test_integer_sub, integer_sub, 3);
    test_op!(IntegerMod, test_integer_mod, integer_mod, 1);
    test_op!(IntegerBitwiseAnd, test_integer_bitwise_and, integer_bitwise_and, 0);
    test_op!(IntegerBitwiseOr, test_integer_bitwise_or, integer_bitwise_or, 7);
    test_op!(IntegerBitwiseXor, test_integer_bitwise_xor, integer_bitwise_xor, 7);
    test_op!(IntegerShiftLeft, test_integer_shift_left, integer_shift_left, 20);
    test_op!(IntegerShiftRight, test_integer_shift_right, integer_shift_right, 1);

    test_bool_op!(IntegerSmaller, test_integer_smaller, integer_smaller, false_object);
    test_bool_op!(IntegerGreater, test_integer_greater, integer_greater, true_object);
    test_bool_op!(IntegerEquals, test_integer_equals, integer_equals, false_object);

    test_cast_op!(IntegerToFloat, test_integer_to_float, integer_to_float, as_float, 5.0);
    test_cast_op!(IntegerToString, test_integer_to_string, integer_to_string, as_string, &"5".to_string());
}
