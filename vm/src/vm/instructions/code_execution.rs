//! VM instruction handlers for executing bytecode files and code objects.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Runs a runtime allocated CompiledCode.
///
/// This instruction takes the following arguments:
///
/// 1. The register to store the return value in.
/// 2. The register containing the CompiledCode object to run.
/// 3. The register containing an array of arguments to pass.
/// 4. The Binding to use, if any. Omitting this argument results in a
///    Binding being created automatically.
pub fn run_code(machine: &Machine,
                process: &RcProcess,
                _: &RcCompiledCode,
                instruction: &Instruction)
                -> InstructionResult {
    process.advance_line(instruction.line);

    let register = instruction.arg(0)?;
    let cc_ptr = process.get_register(instruction.arg(1)?)?;
    let args_ptr = process.get_register(instruction.arg(2)?)?;

    let code_obj = {
        let cc_obj = cc_ptr.get();

        ensure_compiled_code!(instruction, cc_obj);

        cc_obj.value.as_compiled_code()
    };

    let args_obj = args_ptr.get();

    ensure_arrays!(instruction, args_obj);

    let arguments = args_obj.value.as_array();
    let arg_count = arguments.len();

    let binding_idx = 3 + arg_count;

    let binding = if instruction.arg(binding_idx).is_ok() {
        let obj_ptr = process.get_register(binding_idx)?;
        let obj = obj_ptr.get();

        if !obj.value.is_binding() {
            return Err(format!("Argument {} is not a valid Binding",
                               binding_idx));
        }

        Some(obj.value.as_binding())
    } else {
        None
    };

    machine.schedule_code(process.clone(),
                          code_obj,
                          cc_ptr,
                          arguments,
                          binding,
                          register);

    process.pop_call_frame();

    Ok(Action::EnterContext)
}

/// Runs a CompiledCode literal.
///
/// This instruction is meant to execute simple CompiledCode objects,
/// usually the moment they're defined. For more complex use cases see the
/// "run_code" instruction.
///
/// This instruction takes the following arguments:
///
/// 1. The register to store the return value in.
/// 2. The index of the code object to run.
/// 3. The register containing the object to use as "machine" when running the
///    CompiledCode.
pub fn run_literal_code(machine: &Machine,
                        process: &RcProcess,
                        code: &RcCompiledCode,
                        instruction: &Instruction)
                        -> InstructionResult {
    process.advance_line(instruction.line);

    let register = instruction.arg(0)?;
    let code_index = instruction.arg(1)?;
    let receiver = process.get_register(instruction.arg(2)?)?;
    let code_obj = code.code_object(code_index)?;

    machine.schedule_code(process.clone(),
                          code_obj,
                          receiver,
                          &Vec::new(),
                          None,
                          register);

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

    ensure_strings!(instruction, path);

    machine.run_file(path.value.as_string(), process, instruction, register)
}
