//! VM instruction handlers for float operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Sets a float in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the float in.
/// 2. The index of the float literals to use for the value.
///
/// The float literal is extracted from the given CompiledCode.
pub fn set_float(machine: &Machine,
                 process: &RcProcess,
                 code: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    let register = instruction.arg(0)?;
    let index = instruction.arg(1)?;
    let value = *code.float(index)?;

    let obj = process.allocate(object_value::float(value),
                               machine.state.float_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Adds two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to add.
pub fn float_add(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, +)
}

/// Multiplies two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to multiply with.
pub fn float_mul(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, *)
}

/// Divides two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to divide with.
pub fn float_div(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, /)
}

/// Subtracts two floats
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float to subtract.
pub fn float_sub(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, -)
}

/// Gets the modulo of a float
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The register of the float argument.
pub fn float_mod(machine: &Machine,
                 process: &RcProcess,
                 _: &RcCompiledCode,
                 instruction: &Instruction)
                 -> InstructionResult {
    float_op!(machine, process, instruction, %)
}

/// Converts a float to an integer
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
pub fn float_to_integer(machine: &Machine,
                        process: &RcProcess,
                        _: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let register = instruction.arg(0)?;
    let float_ptr = process.get_register(instruction.arg(1)?)?;
    let float = float_ptr.get();

    ensure_floats!(instruction, float);

    let result = float.value.as_float() as i64;

    let obj = process.allocate(object_value::integer(result),
                               machine.state.integer_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Converts a float to a string
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the float to convert.
pub fn float_to_string(machine: &Machine,
                       process: &RcProcess,
                       _: &RcCompiledCode,
                       instruction: &Instruction)
                       -> InstructionResult {
    let register = instruction.arg(0)?;
    let float_ptr = process.get_register(instruction.arg(1)?)?;
    let float = float_ptr.get();

    ensure_floats!(instruction, float);

    let result = float.value.as_float().to_string();

    let obj = process.allocate(object_value::string(result),
                               machine.state.string_prototype.clone());

    process.set_register(register, obj);

    Ok(Action::None)
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
pub fn float_smaller(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    float_bool_op!(machine, process, instruction, <)
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
pub fn float_greater(machine: &Machine,
                     process: &RcProcess,
                     _: &RcCompiledCode,
                     instruction: &Instruction)
                     -> InstructionResult {
    float_bool_op!(machine, process, instruction, >)
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
pub fn float_equals(machine: &Machine,
                    process: &RcProcess,
                    _: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    float_bool_op!(machine, process, instruction, ==)
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

                    let instruction = new_instruction(InstructionType::SetFloat,
                                                      vec![0]);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_invalid_right_register() {
                    let (machine, code, process) = setup();

                    let instruction = new_instruction(InstructionType::$ins_type,
                                                      vec![2, 0, 1]);

                    let left = process
                        .allocate_without_prototype(object_value::float(5.0));

                    process.set_register(0, left);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_valid_arguments() {
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

                    let pointer = process.get_register(2).unwrap();

                    assert_eq!(pointer.get().value.as_float(), $expected);
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

                    let left = process
                        .allocate_without_prototype(object_value::float(5.0));

                    process.set_register(0, left);

                    assert!($ins_func(&machine, &process, &code, &instruction).is_err());
                }

                #[test]
                fn test_with_valid_arguments() {
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

                    let original = process
                        .allocate_without_prototype(object_value::float(5.5));

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

    mod set_float {
        use super::*;

        #[test]
        fn test_without_arguments() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetFloat,
                                              Vec::new());

            let result = set_float(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_invalid_literal_index() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetFloat,
                                              vec![0, 0]);

            let result = set_float(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_valid_arguments() {
            let (machine, code, process) = setup();
            let instruction = new_instruction(InstructionType::SetFloat,
                                              vec![0, 0]);

            arc_mut(&code).float_literals.push(10.0);

            let result = set_float(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            let pointer = process.get_register(0).unwrap();

            assert_eq!(pointer.get().value.as_float(), 10.0);
        }
    }

    test_op!(FloatAdd, float_add, 7.0);
    test_op!(FloatDiv, float_div, 2.5);
    test_op!(FloatMul, float_mul, 10.0);
    test_op!(FloatSub, float_sub, 3.0);
    test_op!(FloatMod, float_mod, 1.0);

    test_bool_op!(FloatSmaller, float_smaller, false_object);
    test_bool_op!(FloatGreater, float_greater, true_object);
    test_bool_op!(FloatEquals, float_equals, false_object);

    test_cast_op!(FloatToInteger, float_to_integer, as_integer, 5);
    test_cast_op!(FloatToString, float_to_string, as_string, &"5.5".to_string());
}
