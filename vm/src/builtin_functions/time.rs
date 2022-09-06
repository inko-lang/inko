//! Functions for system and monotonic clocks.
use crate::date_time::DateTime;
use crate::mem::{Float, Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::state::State;

pub(crate) fn time_monotonic(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let duration = state.start_time.elapsed();
    let seconds = duration.as_secs_f64();

    Ok(Float::alloc(state.permanent_space.float_class(), seconds))
}

pub(crate) fn time_system(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let time = DateTime::now().timestamp();

    Ok(Float::alloc(state.permanent_space.float_class(), time))
}

pub(crate) fn time_system_offset(
    state: &State,
    _: ProcessPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let offset = DateTime::now().utc_offset();

    Ok(Int::alloc(state.permanent_space.int_class(), offset))
}
