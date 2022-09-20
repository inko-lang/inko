//! Functions for Inko processes.
use crate::location_table::Location;
use crate::mem::{Int, Pointer};
use crate::process::ProcessPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process::Thread;
use crate::state::State;

pub(crate) fn process_stacktrace(
    _: &State,
    _: &mut Thread,
    process: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let skip = unsafe { Int::read(arguments[0]) as usize };
    let raw_trace = process.stacktrace();
    let len = raw_trace.len();
    let limit = if skip > 0 { len.saturating_sub(skip) } else { len };
    let trace: Vec<Location> = raw_trace.into_iter().take(limit).collect();

    Ok(Pointer::boxed(trace))
}

pub(crate) fn process_call_frame_name(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let trace = unsafe { arguments[0].get::<Vec<Location>>() };
    let index = unsafe { Int::read(arguments[1]) as usize };

    Ok(trace[index].name)
}

pub(crate) fn process_call_frame_path(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let trace = unsafe { arguments[0].get::<Vec<Location>>() };
    let index = unsafe { Int::read(arguments[1]) as usize };

    Ok(trace[index].file)
}

pub(crate) fn process_call_frame_line(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let trace = unsafe { arguments[0].get::<Vec<Location>>() };
    let index = unsafe { Int::read(arguments[1]) as usize };

    Ok(trace[index].line)
}

pub(crate) fn process_stacktrace_drop(
    _: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { arguments[0].drop_boxed::<Vec<Location>>() }
    Ok(Pointer::nil_singleton())
}

pub(crate) fn process_stacktrace_length(
    state: &State,
    _: &mut Thread,
    _: ProcessPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let trace = unsafe { arguments[0].get::<Vec<Location>>() }.len();

    Ok(Int::alloc(state.permanent_space.int_class(), trace as i64))
}
