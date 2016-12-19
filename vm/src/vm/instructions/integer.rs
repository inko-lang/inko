//! VM instruction handlers for integer operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets an integer in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the integer in.
/// 2. The index of the integer literals to use for the value.
///
/// The integer literal is extracted from the given CompiledCode.
pub fn set_integer(machine: &Machine,
                   process: &RcProcess,
                   code: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let index = instruction.arg(1)?;
    let value = *code.integer(index)?;

    let obj = process.allocate(object_value::integer(value),
                               machine.state.integer_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Adds two integers
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
pub fn integer_add(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    integer_op!(machine, process, instruction, +)
}

/// Divides an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
pub fn integer_div(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    integer_op!(machine, process, instruction, /)
}

/// Multiplies an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
pub fn integer_mul(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    integer_op!(machine, process, instruction, *)
}

/// Subtracts an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
pub fn integer_sub(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    integer_op!(machine, process, instruction, -)
}

/// Gets the modulo of an integer
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the left-hand side object.
/// 3. The register of the right-hand side object.
pub fn integer_mod(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    integer_op!(machine, process, instruction, %)
}

/// Converts an integer to a float
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to convert.
pub fn integer_to_float(machine: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let register = instruction.arg(0)?;
    let integer_ptr = process.get_register(instruction.arg(1)?)?;
    let integer = integer_ptr.get();

    ensure_integers!(instruction, integer);

    let result = integer.value.as_integer() as f64;

    let obj = process.allocate(object_value::float(result),
                               machine.state.float_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Converts an integer to a string
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to convert.
pub fn integer_to_string(machine: &Machine,
                         process: &RcProcess,
                         _: &RcCompiledCode,
                         instruction: &Instruction)
                         -> InstructionResult {
    let register = instruction.arg(0)?;
    let integer_ptr = process.get_register(instruction.arg(1)?)?;
    let integer = integer_ptr.get();

    ensure_integers!(instruction, integer);

    let result = integer.value.as_integer().to_string();

    let obj = process.allocate(object_value::string(result),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Performs an integer bitwise AND.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
pub fn integer_bitwise_and(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    integer_op!(machine, process, instruction, &)
}

/// Performs an integer bitwise OR.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
pub fn integer_bitwise_or(machine: &Machine,
                          process: &RcProcess,
                          _: &RcCompiledCode,
                          instruction: &Instruction)
                          -> InstructionResult {
    integer_op!(machine, process, instruction, |)
}

/// Performs an integer bitwise XOR.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
pub fn integer_bitwise_xor(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    integer_op!(machine, process, instruction, ^)
}

/// Shifts an integer to the left.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
pub fn integer_shift_left(machine: &Machine,
                          process: &RcProcess,
                          _: &RcCompiledCode,
                          instruction: &Instruction)
                          -> InstructionResult {
    integer_op!(machine, process, instruction, <<)
}

/// Shifts an integer to the right.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the integer to operate on.
/// 3. The register of the integer to use as the operand.
pub fn integer_shift_right(machine: &Machine,
                           process: &RcProcess,
                           _: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    integer_op!(machine, process, instruction, >>)
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
pub fn integer_smaller(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    integer_bool_op!(machine, process, instruction, <)
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
pub fn integer_greater(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    integer_bool_op!(machine, process, instruction, >)
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
pub fn integer_equals(machine: &Machine,
                      process: &RcProcess,
                      _: &RcCompiledCode,
                      instruction: &Instruction)
                      -> InstructionResult {
    integer_bool_op!(machine, process, instruction, ==)
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_value;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    macro_rules! test_op {
        ($ins_type: ident, $ins_func: ident, $expected: expr) => (
            mod $ins_func {
                use super::*;

                #[test]
                fn test_without_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      Vec::new());

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_left_register() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::SetInteger,
                                                      vec![0]);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_right_register() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![2, 0, 1]);

                    let left =
                        process.allocate_without_prototype(object_value::integer(5));

                    process.set_register(0, left);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_valid_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![2, 0, 1]);

                    let left =
                        process.allocate_without_prototype(object_value::integer(5));

                    let right =
                        process.allocate_without_prototype(object_value::integer(2));

                    process.set_register(0, left);
                    process.set_register(1, right);

                    let result = $ins_func(&machine, &process, &code, &instruction);

                    assert!(result.is_ok());

                    let pointer = process.get_register(2).unwrap();

                    assert_eq!(pointer.get().value.as_integer(), $expected);
                }
            }
        );
    }

    macro_rules! test_bool_op {
        ($ins_type: ident, $ins_func: ident, $expected: ident) => (
            mod $ins_func {
                use super::*;

                #[test]
                fn test_without_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      Vec::new());

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_left_register() {
                    let (machine, code, process) = setup();

                    let instruction =
                        new_instruction(InstructionType::$ins_type, vec![0]);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_right_register() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![2, 0, 1]);

                    let left =
                        process.allocate_without_prototype(object_value::integer(5));

                    process.set_register(0, left);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_valid_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![2, 0, 1]);

                    let left =
                        process.allocate_without_prototype(object_value::integer(5));

                    let right =
                        process.allocate_without_prototype(object_value::integer(2));

                    process.set_register(0, left);
                    process.set_register(1, right);

                    let result =
                        $ins_func(&machine, &process, &code, &instruction);

                    assert!(result.is_ok());

                    let pointer = process.get_register(2).unwrap();

                    assert!(pointer == machine.state.$expected);
                }
            }
        );
    }

    macro_rules! test_cast_op {
        ($ins_type: ident, $ins_func: ident, $target_type: ident, $target_val: expr) => (
            mod $ins_func {
                use super::*;

                #[test]
                fn test_without_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      Vec::new());

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_without_source_register() {
                    let (machine, code, process) = setup();

                    let instruction =
                        new_instruction(InstructionType::$ins_type, vec![0]);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_source_register() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![1, 0]);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_valid_arguments() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![1, 0]);

                    let original =
                        process.allocate_without_prototype(object_value::integer(5));

                    process.set_register(0, original);

                    let result =
                        $ins_func(&machine, &process, &code, &instruction);

                    assert!(result.is_ok());

                    let pointer = process.get_register(1).unwrap();

                    assert!(pointer.get().value.$target_type() == $target_val);
                }
            }
        );
    }

    mod set_integer {
        use super::*;

        #[test]
        fn test_without_arguments() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetInteger,
                                              Vec::new());

            let result = set_integer(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_invalid_literal_index() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetInteger,
                                              vec![0, 0]);

            let result = set_integer(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_valid_arguments() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetInteger,
                                              vec![0, 0]);

            arc_mut(&code).integer_literals.push(10);

            let result = set_integer(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            let pointer = process.get_register(0).unwrap();

            assert_eq!(pointer.get().value.as_integer(), 10);
        }
    }

    test_op!(IntegerAdd, integer_add, 7);
    test_op!(IntegerDiv, integer_div, 2);
    test_op!(IntegerMul, integer_mul, 10);
    test_op!(IntegerSub, integer_sub, 3);
    test_op!(IntegerMod, integer_mod, 1);
    test_op!(IntegerBitwiseAnd, integer_bitwise_and, 0);
    test_op!(IntegerBitwiseOr, integer_bitwise_or, 7);
    test_op!(IntegerBitwiseXor, integer_bitwise_xor, 7);
    test_op!(IntegerShiftLeft, integer_shift_left, 20);
    test_op!(IntegerShiftRight, integer_shift_right, 1);

    test_bool_op!(IntegerSmaller, integer_smaller, false_object);
    test_bool_op!(IntegerGreater, integer_greater, true_object);
    test_bool_op!(IntegerEquals, integer_equals, false_object);

    test_cast_op!(IntegerToFloat, integer_to_float, as_float, 5.0);
    test_cast_op!(IntegerToString, integer_to_string, as_string, &"5".to_string());
}
