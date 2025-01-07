use crate::context;
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::scheduler::timeouts::Deadline;
use crate::socket::Socket;
use crate::state::State;

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_socket_poll(
    state: *const State,
    mut process: ProcessPointer,
    socket: *mut Socket,
    interest: i64,
    deadline: i64,
) -> bool {
    let interest = if interest == 1 { Interest::Write } else { Interest::Read };
    let state = &*state;
    let socket = &mut *socket;
    let poll_id = unsafe { process.thread() }.network_poller;

    // We must keep the process' state lock open until everything is registered,
    // otherwise a timeout thread may reschedule the process (i.e. the timeout
    // is very short) before we finish registering the socket with a poller.
    {
        let mut proc_state = process.state();

        // A deadline of -1 signals that we should wait indefinitely.
        if deadline >= 0 {
            let time = Deadline::until(deadline as u64);
            let timeout_id = state.timeout_worker.suspend(process, time);

            proc_state.waiting_for_io(Some(timeout_id));
        } else {
            proc_state.waiting_for_io(None);
        }

        socket.register(state, process, poll_id, interest);
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
        socket.deregister(state);
        false
    } else {
        true
    }
}
