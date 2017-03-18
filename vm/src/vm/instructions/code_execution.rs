//! VM instruction handlers for executing bytecode files and code objects.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Executes a CompiledCode object.
///
/// This instruction takes the following arguments:
///
/// 1. The register to store the return value in.
/// 2. The register containing the CompiledCode object to run.
/// 3. The Binding to use, if any. Setting this register to nil will result in a
///    binding being created automatically.
/// 4. A boolean (1 or 0) indicating if the last argument is a rest argument. A
///    rest argument will be unpacked into separate arguments.
///
/// Any extra arguments passed are passed as arguments to the CompiledCode
/// object.
pub fn run_code(machine: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    process.advance_line(instruction.line);

    let register = instruction.arg(0)?;
    let code_ptr = process.get_register(instruction.arg(1)?)?;

    // Figure out what binding we need to use.
    let binding = {
        let ptr = process.get_register(instruction.arg(2)?)?;

        if ptr == machine.state.nil_object {
            None
        } else {
            Some(ptr.get().value.as_binding()?.clone())
        }
    };

    let rest_arg = instruction.arg(3)? == 1;
    let code_obj = code_ptr.get();
    let code_val = code_obj.value.as_compiled_code()?;

    // Argument handling
    let arg_count = instruction.arguments.len() - 4;
    let tot_args = code_val.arguments as usize;
    let req_args = code_val.required_arguments as usize;

    let mut arguments =
        machine.collect_arguments(process.clone(), instruction, 4, arg_count)?;

    // Unpack the last argument if it's a rest argument
    if rest_arg {
        if let Some(last_arg) = arguments.pop() {
            let array = last_arg.get();

            for value in array.value.as_array()? {
                arguments.push(value.clone());
            }
        }
    }

    // If the code object defines a rest argument we'll pack any excessive
    // arguments into a single array.
    if code_val.rest_argument && arguments.len() > tot_args {
        let rest_count = arguments.len() - tot_args;
        let mut rest = Vec::new();

        for obj in arguments[arguments.len() - rest_count..].iter() {
            rest.push(obj.clone());
        }

        arguments.truncate(tot_args);

        let rest_array = process.allocate(object_value::array(rest),
                                          machine.state.array_prototype.clone());

        arguments.push(rest_array);
    } else if code_val.rest_argument && arguments.len() == 0 {
        let rest_array = process.allocate(object_value::array(Vec::new()),
                                          machine.state.array_prototype.clone());

        arguments.push(rest_array);
    }

    if arguments.len() > tot_args && !code_val.rest_argument {
        return Err(format!("{} accepts up to {} arguments, but {} \
                            arguments were given",
                           code_val.name,
                           code_val.arguments,
                           arguments.len()));
    }

    if arguments.len() < req_args {
        return Err(format!("{} requires {} arguments, but {} arguments \
                            were given",
                           code_val.name,
                           code_val.required_arguments,
                           arguments.len()));
    }

    machine.schedule_code(process.clone(), code_val, &arguments, binding, register);

    process.pop_call_frame();

    Ok(Action::EnterContext)
}

/// Parses and runs a given bytecode file using a string literal
///
/// Files are executed only once. After a file has been executed any
/// following calls are basically no-ops.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the resulting object in.
/// 2. The string literal index containing the file path of the bytecode
///    file.
///
/// The result of this instruction is whatever the bytecode file returned.
pub fn run_literal_file(machine: &Machine,
                        process: &RcProcess,
                        code: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    let register = instruction.arg(0)?;
    let index = instruction.arg(1)?;
    let path = code.string(index)?;

    machine.run_file(path, process, instruction, register)
}

/// Parses and runs a given bytecode file using a runtime allocated string
///
/// This instruction takes the same arguments as the "run_literal_file"
/// instruction except instead of using a string literal it uses a register
/// containing a runtime allocated string.
pub fn run_file(machine: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    let register = instruction.arg(0)?;
    let path_ptr = process.get_register(instruction.arg(1)?)?;
    let path = path_ptr.get();

    machine.run_file(path.value.as_string()?, process, instruction, register)
}

#[cfg(test)]
mod tests {
    use super::*;
    use binding::Binding;
    use object_value;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    mod run_code {
        use super::*;

        #[test]
        fn test_without_arguments() {
            let (machine, code, process) = setup();

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, machine.state.nil_object);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![2, 0, 1, 0]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            assert!(process.context().parent.is_some());
            assert!(process.binding().locals().is_empty());
        }

        #[test]
        fn test_with_too_many_arguments() {
            let (machine, code, process) = setup();

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, machine.state.nil_object);
            process.set_register(2, machine.state.true_object);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![4, 0, 1, 0, 2]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_not_enough_arguments() {
            let (machine, code, process) = setup();

            arc_mut(&code).arguments = 2;
            arc_mut(&code).required_arguments = 2;

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, machine.state.nil_object);
            process.set_register(2, machine.state.true_object);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![4, 0, 1, 0, 2]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_err());
        }

        #[test]
        fn test_with_enough_arguments() {
            let (machine, code, process) = setup();

            arc_mut(&code).arguments = 2;

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, machine.state.nil_object);
            process.set_register(2, machine.state.true_object);
            process.set_register(3, machine.state.false_object);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![4, 0, 1, 0, 2, 3]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            assert_eq!(process.binding().locals().len(), 2);

            assert!(process.binding().get_local(0).unwrap() ==
                    machine.state.true_object);

            assert!(process.binding().get_local(1).unwrap() ==
                    machine.state.false_object);
        }

        #[test]
        fn test_with_rest_argument() {
            let (machine, code, process) = setup();

            arc_mut(&code).arguments = 2;
            arc_mut(&code).rest_argument = true;

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, machine.state.nil_object);

            let args =
                process.allocate_without_prototype(object_value::array(vec![machine.state.true_object,
                                                                       machine.state.false_object]));

            process.set_register(2, args);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![4, 0, 1, 1, 2]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            assert_eq!(process.binding().locals().len(), 2);

            assert!(process.binding().get_local(0).unwrap() ==
                    machine.state.true_object);

            assert!(process.binding().get_local(1).unwrap() ==
                    machine.state.false_object);
        }

        #[test]
        fn test_with_binding() {
            let (machine, code, process) = setup();

            let code_ptr = process
                .allocate_without_prototype(object_value::compiled_code(code.clone()));

            let binding = Binding::new();

            binding.set_local(0, code_ptr);

            let binding_ptr =
                process.allocate_without_prototype(object_value::binding(binding.clone()));

            process.set_register(0, code_ptr);
            process.set_register(1, binding_ptr);

            let instruction = new_instruction(InstructionType::RunCode,
                                              vec![2, 0, 1, 0]);

            let result = run_code(&machine, &process, &code, &instruction);

            assert!(result.is_ok());

            assert!(process.binding()
                .parent()
                .unwrap()
                .get_local(0)
                .unwrap() == code_ptr);
        }
    }
}
