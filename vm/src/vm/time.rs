//! VM functions for working with time objects.
use date_time::DateTime;
use object_pointer::ObjectPointer;
use object_value;
use process::RcProcess;
use vm::state::RcState;

pub fn monotonic(state: &RcState, process: &RcProcess) -> ObjectPointer {
    let duration = state.start_time.elapsed();
    let seconds = duration.as_secs() as f64
        + (f64::from(duration.subsec_nanos()) / 1_000_000_000.0);

    process.allocate(object_value::float(seconds), state.float_prototype)
}

pub fn system(state: &RcState, process: &RcProcess) -> ObjectPointer {
    let timestamp = DateTime::now().timestamp();

    process.allocate(object_value::float(timestamp), state.float_prototype)
}

pub fn system_offset() -> ObjectPointer {
    ObjectPointer::integer(DateTime::now().utc_offset())
}

pub fn system_dst(state: &RcState) -> ObjectPointer {
    if DateTime::now().dst_active() {
        state.true_object
    } else {
        state.false_object
    }
}
