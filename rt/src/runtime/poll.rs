use crate::context;
use crate::network_poller::Interest;
use crate::poll::Poll;
use crate::process::{ProcessPointer, ProcessState};
use crate::scheduler::timeouts::Deadline;
use crate::state::State;

/// The bit to use for marking the primary source to poll for.
const PRIM_BIT: u8 = 0b01;

/// The bit to use for marking the secondary source to poll for.
const SEC_BIT: u8 = 0b10;

fn waiting_for_io(
    state: &State,
    process: ProcessPointer,
    process_state: &mut ProcessState,
    deadline: i64,
) {
    // A deadline of -1 signals that we should wait indefinitely.
    if deadline >= 0 {
        let time = Deadline::until(deadline as u64);
        let timeout_id = state.timeout_worker.suspend(process, time);

        process_state.waiting_for_io(Some(timeout_id));
    } else {
        process_state.waiting_for_io(None);
    }
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_poll(
    state: *const State,
    process: ProcessPointer,
    poll: *mut Poll,
    interest: i64,
    deadline: i64,
) -> bool {
    let interest = if interest == 1 { Interest::Write } else { Interest::Read };
    let state = &*state;
    let poll = &mut *poll;

    // We must keep the process' state lock open until everything is registered,
    // otherwise a timeout thread may reschedule the process (i.e. the timeout
    // is very short) before we finish registering the socket with a poller.
    {
        let mut proc_state = process.state();

        waiting_for_io(state, process, &mut proc_state, deadline);
        poll.register(state, process, interest);
    }

    // Safety: the current thread is holding on to the process' run lock, so if
    // the process gets rescheduled onto a different thread, said thread won't
    // be able to use it until we finish this context switch.
    unsafe { context::switch(process) };

    if process.timeout_expired() {
        // The socket is still registered at this point, so we have to
        // deregister first. If we don't and suspend for another IO operation,
        // the poller could end up rescheduling the process multiple times (as
        // there are multiple events still in flight for the process).
        poll.deregister(state);
        false
    } else {
        true
    }
}

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_poll_read_either(
    state: *const State,
    process: ProcessPointer,
    primary: *mut Poll,
    secondary: *mut Poll,
    deadline: i64,
) -> i64 {
    let state = &*state;
    let prim = &mut *primary;
    let sec = &mut *secondary;

    {
        let mut proc_state = process.state();
        let id = process.identifier();

        // We poll using the same process twice but using unique tag bits. This
        // way when we're rescheduled we can determine which source(s) is/are
        // readable.
        let prim_proc = ProcessPointer::new((id | PRIM_BIT as usize) as _);
        let sec_proc = ProcessPointer::new((id | SEC_BIT as usize) as _);

        waiting_for_io(state, process, &mut proc_state, deadline);
        prim.register(state, prim_proc, Interest::Read);
        sec.register(state, sec_proc, Interest::Read);
    }

    // Safety: the current thread is holding on to the process' run lock
    unsafe { context::switch(process) };

    match process.check_timeout_and_take_poll_bits() {
        (true, _) => {
            prim.deregister(state);
            sec.deregister(state);
            0
        }
        // The primary source is readable. We need to deregister the secondary
        // source so the current process won't get scheduled twice.
        (_, PRIM_BIT) => {
            sec.deregister(state);
            1
        }
        // The secondary source is readable.
        (_, SEC_BIT) => {
            prim.deregister(state);
            2
        }
        // Both sources are readable.
        _ => 3,
    }
}
