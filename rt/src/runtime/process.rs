use crate::context;
use crate::mem::{PrimitiveString, TypePointer};
use crate::process::{
    Message, NativeAsyncMethod, Process, ProcessPointer, RescheduleRights,
    StackFrame,
};
use crate::scheduler::process::Action;
use crate::scheduler::timeouts::Deadline;
use crate::state::State;
use std::fmt::Write as _;
use std::process::exit;
use std::str;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// There's no real standard across programs for exit codes. Rust uses 101 so
/// for the sake of "we don't know a better value", we also use 101.
pub(crate) const PANIC_STATUS: i32 = 101;

/// Terminates the current program with an Inko panic (opposed to a panic
/// triggered using the `panic!` macro).
///
/// This function is marked as cold as we expect it to be called rarely, if ever
/// (in a correct program). This should also ensure any branches leading to this
/// function are treated as unlikely.
#[inline(never)]
#[cold]
pub(crate) fn panic(process: ProcessPointer, message: &str) -> ! {
    let mut buffer = String::new();

    buffer.push_str("Stack trace (the most recent call comes last):");

    for frame in process.stacktrace() {
        let _ = if !frame.path.is_empty() && frame.line > 0 {
            write!(
                buffer,
                "\n  {}:{} in {}",
                frame.path, frame.line, frame.name,
            )
        } else {
            write!(buffer, "\n  ?? in {}", frame.name)
        };
    }

    let _ = write!(
        buffer,
        "\nProcess '{}' ({:#x}) panicked: {}",
        unsafe { process.header.instance_of.name() },
        process.identifier(),
        message
    );

    eprintln!("{}", buffer);
    exit(PANIC_STATUS);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_panic(
    process: ProcessPointer,
    message: PrimitiveString,
) {
    panic(process, message.as_str());
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_new(
    instance_of: TypePointer,
) -> ProcessPointer {
    Process::alloc(instance_of)
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_send_message(
    state: *const State,
    mut sender: ProcessPointer,
    mut receiver: ProcessPointer,
    method: NativeAsyncMethod,
    data: *mut u8,
) {
    let message = Message { method, data };
    let state = &*state;
    let reschedule = match receiver.send_message(message) {
        RescheduleRights::AcquiredWithTimeout(id) => {
            state.timeout_worker.expire(id);
            true
        }
        RescheduleRights::Acquired => true,
        RescheduleRights::Failed => false,
    };

    if reschedule {
        sender.thread().schedule(receiver);
    }
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_finish_message(
    mut process: ProcessPointer,
    terminate: bool,
) {
    let resched = process.finish_message();

    if terminate {
        // Safety: we can't terminate the process here as that would result in
        // us corrupting the current stack (= the process' stack), so instead we
        // defer this until we switch back to the thread's stack.
        process.thread().action = Action::Terminate;
    } else if resched {
        process.thread().schedule(process);
    }

    context::switch(process);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_yield(mut process: ProcessPointer) {
    // Safety: the current thread is holding on to the run lock
    process.thread().schedule(process);
    context::switch(process);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_suspend(
    state: *const State,
    process: ProcessPointer,
    nanos: i64,
) {
    let state = &*state;
    let timeout = Deadline::duration(state, Duration::from_nanos(nanos as _));

    {
        // We need to hold on to the lock until the end as to ensure the process
        // is rescheduled if the timeout happens to expire before we finish the
        // work here.
        let mut proc_state = process.state();
        let timeout_id = state.timeout_worker.suspend(process, timeout);

        proc_state.suspend(timeout_id);
    }

    // Safety: the current thread is holding on to the run lock
    context::switch(process);

    // We need to clear the timeout flag, otherwise future operations may time
    // out promaturely.
    process.clear_timeout();
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stacktrace(
    process: ProcessPointer,
) -> *mut Vec<StackFrame> {
    Box::into_raw(Box::new(process.stacktrace()))
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_name(
    trace: *const Vec<StackFrame>,
    index: i64,
) -> PrimitiveString {
    let val = &(&(*trace)).get_unchecked(index as usize).name;

    PrimitiveString::borrowed(val)
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_path(
    trace: *const Vec<StackFrame>,
    index: i64,
) -> PrimitiveString {
    let val = &(&(*trace)).get_unchecked(index as usize).path;

    PrimitiveString::borrowed(val)
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stack_frame_line(
    trace: *const Vec<StackFrame>,
    index: i64,
) -> i64 {
    (&(*trace)).get_unchecked(index as usize).line
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stacktrace_size(
    trace: *const Vec<StackFrame>,
) -> i64 {
    (*trace).len() as i64
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stacktrace_drop(
    trace: *mut Vec<StackFrame>,
) {
    drop(Box::from_raw(trace));
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_start_blocking(
    process: ProcessPointer,
) {
    process.start_blocking();
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_stop_blocking(
    process: ProcessPointer,
) {
    process.stop_blocking();
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_wait_for_value(
    process: ProcessPointer,
    lock: *const AtomicU8,
    current: u8,
    new: u8,
) {
    let mut state = process.state();

    state.waiting_for_value(None);

    let _ = (*lock).compare_exchange(
        current,
        new,
        Ordering::AcqRel,
        Ordering::Acquire,
    );

    drop(state);
    context::switch(process);
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_wait_for_value_until(
    state: *const State,
    process: ProcessPointer,
    lock: *const AtomicU8,
    current: u8,
    new: u8,
    nanos: u64,
) -> bool {
    let state = &*state;
    let deadline = Deadline::until(nanos);
    let mut proc_state = process.state();
    let _ = (*lock).compare_exchange(
        current,
        new,
        Ordering::AcqRel,
        Ordering::Acquire,
    );

    let timeout_id = state.timeout_worker.suspend(process, deadline);

    proc_state.waiting_for_value(Some(timeout_id));
    drop(proc_state);

    // Safety: the current thread is holding on to the run lock
    context::switch(process);
    process.timeout_expired()
}

#[no_mangle]
pub unsafe extern "system" fn inko_process_reschedule_for_value(
    state: *const State,
    mut process: ProcessPointer,
    waiter: ProcessPointer,
) {
    let state = &*state;

    // Acquiring the rights first _then_ matching on then ensures we don't
    // deadlock with the timeout worker.
    let rights = waiter.state().try_reschedule_for_value();
    let reschedule = match rights {
        RescheduleRights::Failed => false,
        RescheduleRights::Acquired => true,
        RescheduleRights::AcquiredWithTimeout(id) => {
            state.timeout_worker.expire(id);
            true
        }
    };

    if reschedule {
        process.thread().schedule(waiter);
    }
}
