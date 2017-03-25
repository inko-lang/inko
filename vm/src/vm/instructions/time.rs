//! VM instruction handlers for time operations.
use vm::action::Action;
use vm::instruction::Instruction;
use vm::instructions::result::InstructionResult;
use vm::machine::Machine;

use compiled_code::RcCompiledCode;
use object_value;
use process::RcProcess;

/// Gets the current value of a monotonic clock in milliseconds.
///
/// This instruction requires one argument: the register to set the time in, as
/// a float.
pub fn monotonic_time_milliseconds(machine: &Machine,
                                   process: &RcProcess,
                                   _: &RcCompiledCode,
                                   instruction: &Instruction)
                                   -> InstructionResult {
    let register = instruction.arg(0)?;
    let duration = machine.state.start_time.elapsed();

    let msec = (duration.as_secs() * 1_000) as f64 +
               duration.subsec_nanos() as f64 / 1_000_000.0;

    let obj = process.allocate(object_value::float(msec),
                               machine.state.float_prototype);

    process.set_register(register, obj);

    Ok(Action::None)
}

/// Gets the current value of a monotonic clock in nanoseconds.
///
/// This instruction requires one argument: the register to set the time in, as
/// an integer.
pub fn monotonic_time_nanoseconds(machine: &Machine,
                                  process: &RcProcess,
                                  _: &RcCompiledCode,
                                  instruction: &Instruction)
                                  -> InstructionResult {
    let register = instruction.arg(0)?;
    let duration = machine.state.start_time.elapsed();
    let nsec = (duration.as_secs() * 1000000000) + duration.subsec_nanos() as u64;

    let obj = process.allocate(object_value::integer(nsec as i64),
                               machine.state.integer_prototype);

    process.set_register(register, obj);

    Ok(Action::None)
}
