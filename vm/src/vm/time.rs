//! VM functions for working with time objects.
use crate::date_time::DateTime;
use crate::duration;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::vm::state::RcState;

pub fn monotonic(state: &RcState, process: &RcProcess) -> ObjectPointer {
    let duration = state.start_time.elapsed();
    let seconds = duration::to_f64(Some(duration));

    process.allocate(object_value::float(seconds), state.float_prototype)
}

pub fn system(state: &RcState, process: &RcProcess) -> ObjectPointer {
    let dt = DateTime::now();
    let timestamp = process
        .allocate(object_value::float(dt.timestamp()), state.float_prototype);

    let offset = ObjectPointer::integer(dt.utc_offset());

    process.allocate(
        object_value::array(vec![timestamp, offset]),
        state.array_prototype,
    )
}
