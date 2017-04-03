//! VM instruction handlers for float operations.
use vm::instruction::Instruction;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use object_pointer::ObjectPointer;
use process::RcProcess;

/// Sets a float in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the float in.
/// 2. The index of the float literals to use for the value.
#[inline(always)]
pub fn set_float(_: &Machine,
                 process: &RcProcess,
                 code: &RcCompiledCode,
                 instruction: &Instruction) {
    let register = instruction.arg(0);
    let index = instruction.arg(1);

    process.set_register(register, code.float(index));
}

/// Adds two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to add.
#[inline(always)]
pub fn float_add(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    float_op!(machine, process, instruction, +);
}

/// Multiplies two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to multiply with.
#[inline(always)]
pub fn float_mul(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    float_op!(machine, process, instruction, *);
}

/// Divides two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to divide with.
#[inline(always)]
pub fn float_div(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    float_op!(machine, process, instruction, /);
}

/// Subtracts two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to subtract.
#[inline(always)]
pub fn float_sub(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    float_op!(machine, process, instruction, -);
}

/// Gets the modulo of a float
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float argument.
#[inline(always)]
pub fn float_mod(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction) {
    float_op!(machine, process, instruction, %);
}

/// Converts a float to an integer
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
#[inline(always)]
pub fn float_to_integer(_: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction) {
    let register = instruction.arg(0);
    let float_ptr = process.get_register(instruction.arg(1));
    let result = float_ptr.float_value().unwrap() as i64;

    process.set_register(register, ObjectPointer::integer(result));
}

/// Converts a float to a string
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
#[inline(always)]
pub fn float_to_string(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction) {
    let register = instruction.arg(0);
    let float_ptr = process.get_register(instruction.arg(1));
    let result = float_ptr.float_value().unwrap().to_string();

    let obj =
        process.allocate(object_value::string(result),
                         machine.state.string_prototype);

    process.set_register(register, obj);
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
#[inline(always)]
pub fn float_smaller(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction) {
    float_bool_op!(machine, process, instruction, <);
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
#[inline(always)]
pub fn float_greater(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction) {
    float_bool_op!(machine, process, instruction, >);
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
#[inline(always)]
pub fn float_equals(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction) {
    float_bool_op!(machine, process, instruction, ==);
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_value;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    macro_rules! test_op {
        ($ins_type: ident, $test_func: ident, $ins_func: ident, $expected: expr) => (
            #[test]
            fn $test_func() {
                let (machine, code, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![2, 0, 1]);

                let left = process
                    .allocate_without_prototype(object_value::float(5.0));

                let right = process
                    .allocate_without_prototype(object_value::float(2.0));

                process.set_register(0, left);
                process.set_register(1, right);

                let result = $ins_func(&machine, &process, &code, &instruction);

                assert!(result.is_ok());

                let pointer = process.get_register(2);

                assert_eq!(pointer.float_value().unwrap(), $expected);
            }
        );
    }

    macro_rules! test_bool_op {
        ($ins_type: ident, $test_func: ident, $ins_func: ident, $expected: ident) => (
            #[test]
            fn $test_func() {
                let (machine, code, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![2, 0, 1]);

                let left = process
                    .allocate_without_prototype(object_value::float(5.0));

                let right = process
                    .allocate_without_prototype(object_value::float(2.0));

                process.set_register(0, left);
                process.set_register(1, right);

                let result =
                    $ins_func(&machine, &process, &code, &instruction);

                assert!(result.is_ok());

                let pointer = process.get_register(2);

                assert!(pointer == machine.state.$expected);
            }
        );
    }

    macro_rules! test_cast_op {
        ($ins_type: ident, $test_func: ident, $ins_func: ident, $target_type: ident, $target_val: expr) => (
            #[test]
            fn $test_func() {
                let (machine, code, process) = setup();

                let instruction = new_instruction(InstructionType::$ins_type,
                                                  vec![1, 0]);

                let original = process
                    .allocate_without_prototype(object_value::float(5.5));

                process.set_register(0, original);

                let result =
                    $ins_func(&machine, &process, &code, &instruction);

                assert!(result.is_ok());

                let pointer = process.get_register(1);

                assert!(pointer.$target_type().unwrap() == $target_val);
            }
        );
    }

    #[test]
    fn test_set_float() {
        let (machine, code, process) = setup();
        let instruction = new_instruction(InstructionType::SetFloat, vec![0, 0]);

        let float = machine.state.allocate_permanent_float(10.0);

        arc_mut(&code).float_literals.push(float);

        let result = set_float(&machine, &process, &code, &instruction);

        assert!(result.is_ok());

        let pointer = process.get_register(0);

        assert!(pointer == float);
    }

    test_op!(FloatAdd, test_float_add, float_add, 7.0);
    test_op!(FloatDiv, test_float_div, float_div, 2.5);
    test_op!(FloatMul, test_float_mul, float_mul, 10.0);
    test_op!(FloatSub, test_float_sub, float_sub, 3.0);
    test_op!(FloatMod, test_float_mod, float_mod, 1.0);

    test_bool_op!(FloatSmaller,
                  test_float_smaller,
                  float_smaller,
                  false_object);

    test_bool_op!(FloatGreater, test_float_greater, float_greater, true_object);
    test_bool_op!(FloatEquals, test_float_equals, float_equals, false_object);

    test_cast_op!(FloatToInteger,
                  test_float_to_integer,
                  float_to_integer,
                  integer_value,
                  5);

    test_cast_op!(FloatToString,
                  test_float_to_string,
                  float_to_string,
                  string_value,
                  &"5.5".to_string());
}
