use crate::context;
use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::scheduler::timeouts::Timeout;
use crate::socket::Socket;
use crate::state::State;
use std::io::{self};

pub(crate) fn poll(
    state: &State,
    mut process: ProcessPointer,
    socket: &mut Socket,
    interest: Interest,
    deadline: i64,
) -> io::Result<()> {
    let poll_id = unsafe { process.thread() }.network_poller;

    // We must keep the process' state lock open until everything is registered,
    // otherwise a timeout thread may reschedule the process (i.e. the timeout
    // is very short) before we finish registering the socket with a poller.
    {
        let mut proc_state = process.state();

        // A deadline of -1 signals that we should wait indefinitely.
        if deadline >= 0 {
            let time = Timeout::until(deadline as u64);

            proc_state.waiting_for_io(Some(time.clone()));
            state.timeout_worker.suspend(process, time);
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
        return Err(io::Error::from(io::ErrorKind::TimedOut));
    }

    Ok(())
}
