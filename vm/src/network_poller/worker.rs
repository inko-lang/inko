use crate::arc_without_weak::ArcWithoutWeak;
use crate::network_poller::Events;
use crate::vm::state::RcState;

/// The maximum number of events to process in a single poll loop iteration.
const EVENTS_PER_ITERATION: usize = 1024;

pub struct Worker {
    state: RcState,
}

impl Worker {
    pub fn new(state: RcState) -> Self {
        Worker { state }
    }

    pub fn run(&self) {
        let mut events = Events::with_capacity(EVENTS_PER_ITERATION);

        loop {
            if self.state.network_poller.poll(&mut events).is_err() {
                // The poller might be interrupted when shutting down, or when
                // another thread panics. In this case we just shut down
                // gracefully.
                return;
            }

            for id in events.event_ids() {
                let process =
                    unsafe { ArcWithoutWeak::from_raw(id.value() as *mut _) };

                self.state.scheduler.schedule(process);
            }
        }
    }
}
