//! VM instruction handlers for executing bytecode files and code objects.
use binding::Binding;
use block::Block;
use execution_context::ExecutionContext;
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Executes a Block object.
///
/// This instruction takes the following arguments:
///
/// 1. The register to store the return value in.
/// 2. The register containing the Block object to run.
///
/// Any extra arguments passed are passed as arguments to the CompiledCode
/// object.
#[inline(always)]
pub fn run_block(process: &RcProcess, instruction: &Instruction) {
    process.advance_line(instruction.line);

    let register = instruction.arg(0);
    let block_ptr = process.get_register(instruction.arg(1));
    let block_val = block_ptr.block_value().unwrap();

    let arg_offset = 2;
    let arg_count = instruction.arguments.len() - arg_offset;
    let tot_args = block_val.arguments();
    let req_args = block_val.required_arguments();

    if arg_count > tot_args {
        panic!("{} accepts up to {} arguments, but {} arguments were given",
               block_val.name(),
               tot_args,
               arg_count);
    }

    if arg_count < req_args {
        panic!("{} requires {} arguments, but {} arguments were given",
               block_val.name(),
               req_args,
               arg_count);
    }

    let context = ExecutionContext::from_block(block_val, Some(register));

    {
        // Add the arguments to the binding
        let mut locals = context.binding.locals_mut();

        for index in arg_offset..(arg_offset + arg_count) {
            let local_index = index - arg_offset;

            locals[local_index] = process.get_register(instruction.arg(index));
        }
    }

    process.push_context(context);
}

/// Executes a Block object with a rest argument.
///
/// This instruction takes the following arguments:
///
/// 1. The register to store the return value in.
/// 2. The register containing the Block object to run.
///
/// Any extra arguments passed are passed as arguments to the CompiledCode
/// object. If excessive arguments are given they are packed into the block's
/// rest argument.
#[inline(always)]
pub fn run_block_with_rest() {
    // TODO: implement
    //let register = instruction.arg(0);
    //let block_ptr = process.get_register(instruction.arg(1));
    //let block_val = block_ptr.block_value()?;
    //let has_rest = block_val.has_rest_argument();

    // Unpack the last argument if it's a rest argument
    //if rest_arg {
    //if let Some(last_arg) = arguments.pop() {
    //for value in last_arg.array_value()? {
    //arguments.push(value.clone());
    //}
    //}
    //}

    // If the code object defines a rest argument we'll pack any excessive
    // arguments into a single array.
    //if block_val.has_rest_argument() && arguments.len() > tot_args {
    //let rest_count = arguments.len() - tot_args;
    //let mut rest = Vec::new();

    //for obj in arguments[arguments.len() - rest_count..].iter() {
    //rest.push(obj.clone());
    //}

    //arguments.truncate(tot_args);

    //let rest_array = process.allocate(object_value::array(rest),
    //machine.state.array_prototype.clone());

    //arguments.push(rest_array);
    //} else if block_val.has_rest_argument() && arguments.len() == 0 {
    //let rest_array = process.allocate(object_value::array(Vec::new()),
    //machine.state.array_prototype.clone());

    //arguments.push(rest_array);
    //}
}

/// Parses a bytecode file and stores the resulting Block in the register.
///
/// This instruction requires 2 arguments:
///
/// 1. The register to store the resulting Block in.
/// 2. The register containing the file path to open, as a string.
///
/// This instruction will panic if the file does not exist or when the bytecode
/// is invalid.
#[inline(always)]
pub fn parse_file(machine: &Machine,
                  process: &RcProcess,
                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let path_ptr = process.get_register(instruction.arg(1));
    let path_str = path_ptr.string_value().unwrap();

    let block = {
        let mut registry = write_lock!(machine.module_registry);
        let module = registry.get_or_set(path_str)
            .map_err(|err| err.message())
            .unwrap();

        Block::new(module.code(),
                   Binding::new(module.code.locals()),
                   module.global_scope_ref())
    };

    let block_ptr = process.allocate(object_value::block(block),
                                    machine.state.block_prototype);

    process.set_register(register, block_ptr);
}

/// Sets the target register to true if the given file path has been parsed.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the resulting boolean in.
/// 2. The register containing the file path to check.
///
/// The result of this instruction is true or false.
#[inline(always)]
pub fn file_parsed(machine: &Machine,
                   process: &RcProcess,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let path_ptr = process.get_register(instruction.arg(1));
    let path_str = path_ptr.string_value().unwrap();

    let ptr = if read_lock!(machine.module_registry).contains_path(path_str) {
        machine.state.true_object
    } else {
        machine.state.false_object
    };

    process.set_register(register, ptr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_value;
    use vm::instruction::InstructionType;
    use vm::instructions::test::*;

    mod run_block {
        use super::*;

        #[test]
        fn test_without_arguments() {
            let (_machine, block, process) = setup();

            let block_ptr =
                process.allocate_without_prototype(object_value::block(block));

            process.set_register(0, block_ptr);

            let instruction = new_instruction(InstructionType::RunBlock,
                                              vec![1, 0]);

            run_block(&process, &instruction);

            assert!(process.context().parent.is_some());
            assert_eq!(process.binding().local_exists(0), false);
        }

        #[test]
        #[should_panic]
        fn test_with_too_many_arguments() {
            let (machine, block, process) = setup();

            let block_ptr =
                process.allocate_without_prototype(object_value::block(block));

            process.set_register(0, block_ptr);
            process.set_register(1, machine.state.true_object);

            let instruction = new_instruction(InstructionType::RunBlock,
                                              vec![2, 0, 1]);

            run_block(&process, &instruction);
        }

        #[test]
        #[should_panic]
        fn test_with_not_enough_arguments() {
            let (machine, mut block, process) = setup();

            block.code.arguments = 2;
            block.code.required_arguments = 2;

            let block_ptr =
                process.allocate_without_prototype(object_value::block(block));

            process.set_register(0, block_ptr);
            process.set_register(1, machine.state.true_object);

            let instruction = new_instruction(InstructionType::RunBlock,
                                              vec![2, 0, 1]);

            run_block(&process, &instruction);
        }

        #[test]
        fn test_with_enough_arguments() {
            let (machine, mut block, process) = setup();

            block.code.arguments = 2;

            let block_ptr =
                process.allocate_without_prototype(object_value::block(block));

            process.set_register(0, block_ptr);
            process.set_register(1, machine.state.true_object);
            process.set_register(2, machine.state.false_object);

            let instruction = new_instruction(InstructionType::RunBlock,
                                              vec![3, 0, 1, 2]);

            run_block(&process, &instruction);

            assert!(process.binding().local_exists(0));
            assert!(process.binding().local_exists(1));

            assert!(process.binding().get_local(0) == machine.state.true_object);
            assert!(process.binding().get_local(1) == machine.state.false_object);
        }
    }
}
