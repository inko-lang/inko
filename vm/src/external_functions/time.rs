//! Functions for system and monotonic clocks.
use crate::date_time::DateTime;
use crate::duration;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

/// Returns the current time using a monotonically increasing clock.
pub fn time_monotonic(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let duration = state.start_time.elapsed();
    let seconds = duration::to_f64(Some(duration));
    let res =
        process.allocate(object_value::float(seconds), state.float_prototype);

    Ok(res)
}

/// Returns the current system time.
pub fn time_system(
    state: &RcState,
    process: &RcProcess,
    _: &[ObjectPointer],
) -> Result<ObjectPointer, RuntimeError> {
    let dt = DateTime::now();
    let timestamp = process
        .allocate(object_value::float(dt.timestamp()), state.float_prototype);

    let offset = ObjectPointer::integer(dt.utc_offset());

    let res = process.allocate(
        object_value::array(vec![timestamp, offset]),
        state.array_prototype,
    );

    Ok(res)
}

register!(time_monotonic, time_system);
