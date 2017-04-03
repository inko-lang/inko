//! VM instruction handlers for time operations.
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Gets the current value of a monotonic clock in milliseconds.
///
/// This instruction requires one argument: the register to set the time in, as
/// a float.
#[inline(always)]
pub fn monotonic_time_milliseconds(machine: &Machine,
                                   process: &RcProcess,
                                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let duration = machine.state.start_time.elapsed();

    let msec = (duration.as_secs() * 1_000) as f64 +
               duration.subsec_nanos() as f64 / 1_000_000.0;

    let obj = process.allocate(object_value::float(msec),
                               machine.state.float_prototype);

    process.set_register(register, obj);
}

/// Gets the current value of a monotonic clock in nanoseconds.
///
/// This instruction requires one argument: the register to set the time in, as
/// an integer.
#[inline(always)]
pub fn monotonic_time_nanoseconds(machine: &Machine,
                                  process: &RcProcess,
                                  instruction: &Instruction) {
    let register = instruction.arg(0);
    let duration = machine.state.start_time.elapsed();
    let nsec = (duration.as_secs() * 1000000000) + duration.subsec_nanos() as u64;

    process.set_register(register, ObjectPointer::integer(nsec as i64));
}
