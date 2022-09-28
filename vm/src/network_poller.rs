//! Polling of non-blocking sockets using the system's polling mechanism.
use crate::process::{ProcessPointer, RescheduleRights};
use crate::state::RcState;
use polling::{Event, Poller, Source};
use std::io;

/// The type of event a poller should wait for.
pub(crate) enum Interest {
    /// We're only interested in read operations.
    Read,

    /// We're only interested in write operations.
    Write,
}

/// A poller for non-blocking sockets.
pub(crate) struct NetworkPoller {
    poller: Poller,
}

impl NetworkPoller {
    pub(crate) fn new() -> Self {
        NetworkPoller {
            poller: Poller::new().expect("Failed to set up the network poller"),
        }
    }

    pub(crate) fn poll(&self, events: &mut Vec<Event>) -> io::Result<usize> {
        self.poller.wait(events, None)
    }

    pub(crate) fn add(
        &self,
        process: ProcessPointer,
        source: impl Source,
        interest: Interest,
    ) -> io::Result<()> {
        self.poller.add(source, self.event(process, interest))
    }

    pub(crate) fn modify(
        &self,
        process: ProcessPointer,
        source: impl Source,
        interest: Interest,
    ) -> io::Result<()> {
        self.poller.modify(source, self.event(process, interest))
    }

    pub(crate) fn delete(&self, source: impl Source) -> io::Result<()> {
        self.poller.delete(source)
    }

    fn event(&self, process: ProcessPointer, interest: Interest) -> Event {
        let key = process.identifier();

        match interest {
            Interest::Read => Event::readable(key),
            Interest::Write => Event::writable(key),
        }
    }
}

/// A thread that polls a poller and reschedules processes.
pub(crate) struct Worker {
    id: usize,
    state: RcState,
}

impl Worker {
    pub(crate) fn new(id: usize, state: RcState) -> Self {
        Worker { id, state }
    }

    pub(crate) fn run(&self) {
        let mut events = Vec::new();
        let poller = &self.state.network_pollers[self.id];

        loop {
            if let Err(err) = poller.poll(&mut events) {
                if err.kind() != io::ErrorKind::Interrupted {
                    // It's not entirely clear if/when we ever run into this,
                    // but should we run into any error that's _not_ an
                    // interrupt then there's probably more going on, and all we
                    // can do is abort.
                    panic!("Polling for IO events failed: {:?}", err);
                }
            }

            let processes = events
                .iter()
                .filter_map(|ev| {
                    let proc = unsafe { ProcessPointer::new(ev.key as *mut _) };
                    let mut state = proc.state();

                    // A process may have also been registered with the timeout
                    // thread (e.g. when using a timeout). As such we should
                    // only reschedule the process if the timout thread didn't
                    // already do this for us.
                    match state.try_reschedule_for_io() {
                        RescheduleRights::Failed => None,
                        RescheduleRights::Acquired => Some(proc),
                        RescheduleRights::AcquiredWithTimeout => {
                            self.state
                                .timeout_worker
                                .increase_expired_timeouts();
                            Some(proc)
                        }
                    }
                })
                .collect();

            self.state.scheduler.schedule_multiple(processes);
            events.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_process_class, new_process};
    use std::net::UdpSocket;
    use std::time::Duration;

    #[test]
    fn test_add() {
        let class = empty_process_class("A");
        let process = new_process(*class);
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        assert!(poller.add(*process, &output, Interest::Read).is_ok());
    }

    #[test]
    fn test_modify() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let class = empty_process_class("A");
        let process = new_process(*class);

        assert!(poller.add(*process, &output, Interest::Read).is_ok());
        assert!(poller.modify(*process, &output, Interest::Write).is_ok());
    }

    #[test]
    fn test_delete() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let class = empty_process_class("A");
        let process = new_process(*class);
        let mut events = Vec::with_capacity(1);

        assert!(poller.add(*process, &output, Interest::Write).is_ok());
        assert!(poller.delete(&output).is_ok());

        let len = poller
            .poller
            .wait(&mut events, Some(Duration::from_millis(10)))
            .unwrap();

        assert_eq!(len, 0);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_poll() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let class = empty_process_class("A");
        let process = new_process(*class);
        let mut events = Vec::with_capacity(1);

        poller.add(*process, &output, Interest::Write).unwrap();

        assert!(poller.poll(&mut events).is_ok());
        assert_eq!(events.capacity(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, process.identifier());
    }

    #[test]
    fn test_poll_with_lower_capacity() {
        let sock1 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let sock2 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let class = empty_process_class("A");
        let process = new_process(*class);
        let mut events = Vec::with_capacity(1);

        poller.add(*process, &sock1, Interest::Write).unwrap();
        poller.add(*process, &sock2, Interest::Write).unwrap();

        assert!(poller.poll(&mut events).is_ok());
        assert!(events.capacity() >= 2);
        assert_eq!(events.len(), 2);
    }
}
