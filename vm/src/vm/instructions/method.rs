//! VM instruction handlers for method operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Defines a method for an object.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method object in.
/// 2. The register pointing to a specific object to define the method
///    on.
/// 3. The register containing a String to use as the method name.
/// 4. The register containing the CompiledCode object to use for the
///    method.
pub fn def_method(machine: &Machine,
                  process: &RcProcess,
                  _: &RcCompiledCode,
                  instruction: &Instruction)
                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let receiver_ptr = process.get_register(instruction.arg(1)?)?;
    let name_ptr = process.get_register(instruction.arg(2)?)?;
    let cc_ptr = process.get_register(instruction.arg(3)?)?;
    let name_obj = name_ptr.get();

    let cc_obj = cc_ptr.get();
    let name = name_obj.value.as_string()?;
    let cc = cc_obj.value.as_compiled_code()?;

    let method = machine.allocate_method(&process, &receiver_ptr, cc);

    receiver_ptr.add_method(&process, name.clone(), method);

    process.set_register(register, method);

    Ok(Action::None)
}

/// Defines a method for an object using literals.
///
/// This instruction can be used to define a method when the name and the
/// compiled code object are defined as literals. This instruction is
/// primarily meant to define methods that are defined directly in the
/// source code. Methods defined during runtime should be created using the
/// `def_method` instruction instead.
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the method object in.
/// 2. The register pointing to the object to define the method on.
/// 3. The string literal index to use for the method name.
/// 4. The code object index to use for the method's CompiledCode object.
pub fn def_literal_method(machine: &Machine,
                          process: &RcProcess,
                          code: &RcCompiledCode,
                          instruction: &Instruction)
                          -> InstructionResult {
    let register = instruction.arg(0)?;
    let receiver_ptr = process.get_register(instruction.arg(1)?)?;
    let name_index = instruction.arg(2)?;
    let cc_index = instruction.arg(3)?;

    let name = code.string(name_index)?;
    let cc = code.code_object(cc_index)?;
    let method = machine.allocate_method(&process, &receiver_ptr, cc);

    receiver_ptr.add_method(&process, name.clone(), method);

    process.set_register(register, method);

    Ok(Action::None)
}

/// Sends a message using a string literal
///
/// This instruction requires at least 4 arguments:
///
/// 1. The register to store the result in.
/// 2. The register of the receiver.
/// 3. The index of the string literal to use for the method name.
/// 4. A boolean (1 or 0) to indicate if the last argument is a rest
///    argument. A rest argument will be unpacked into separate arguments.
///
/// Any extra instruction arguments will be passed as arguments to the
/// method.
pub fn send_literal(machine: &Machine,
                    process: &RcProcess,
                    code: &RcCompiledCode,
                    instruction: &Instruction)
                    -> InstructionResult {
    let name_index = instruction.arg(2)?;
    let name = code.string(name_index)?;

    machine.send_message(name, process, instruction)
}

/// Sends a message using a runtime allocated string
///
/// This instruction takes the same arguments as the "send_literal"
/// instruction except instead of the 3rd argument pointing to a string
/// literal it should point to a register containing a string.
pub fn send(machine: &Machine,
            process: &RcProcess,
            _: &RcCompiledCode,
            instruction: &Instruction)
            -> InstructionResult {
    let string = process.get_register(instruction.arg(2)?)?;
    let string_obj = string.get();

    machine.send_message(string_obj.value.as_string()?, process, instruction)
}

/// Checks if an object responds to a message
///
/// This instruction requires 3 arguments:
///
/// 1. The register to store the result in (true or false)
/// 2. The register containing the object to check
/// 3. The string literal index to use as the method name
pub fn literal_responds_to(machine: &Machine,
                           process: &RcProcess,
                           code: &RcCompiledCode,
                           instruction: &Instruction)
                           -> InstructionResult {
    let register = instruction.arg(0)?;
    let source = process.get_register(instruction.arg(1)?)?;
    let name_index = instruction.arg(2)?;
    let name = code.string(name_index)?;

    let source_obj = source.get();

    let result = if source_obj.responds_to(name) {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);

    Ok(Action::None)
}

/// Checks if an object responds to a message using a runtime allocated
/// string.
///
/// This instruction requires the same arguments as the
/// "literal_responds_to" instruction except the last argument should be a
/// register containing a string.
pub fn responds_to(machine: &Machine,
                   process: &RcProcess,
                   _: &RcCompiledCode,
                   instruction: &Instruction)
                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let source = process.get_register(instruction.arg(1)?)?;
    let name = process.get_register(instruction.arg(2)?)?;

    let name_obj = name.get();
    let source_obj = source.get();

    let result = if source_obj.responds_to(name_obj.value.as_string()?) {
        machine.state.true_object.clone()
    } else {
        machine.state.false_object.clone()
    };

    process.set_register(register, result);

    Ok(Action::None)
}
