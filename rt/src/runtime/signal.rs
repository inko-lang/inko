use crate::context;
use crate::process::ProcessPointer;
use crate::state::State;

#[no_mangle]
pub(crate) unsafe extern "system" fn inko_signal_wait(
    state: *const State,
    process: ProcessPointer,
    signal: i64,
) {
    (*state).signals.register(process, signal as _);

    // Safety: the current thread is holding on to the run lock. If we get
    // rescheduled immediately due to a signal, the rescheduled version of the
    // current process will wait until the run lock is released.
    context::switch(process);
}
