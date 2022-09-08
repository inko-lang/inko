//! Polling of non-blocking sockets using the system's polling mechanism.
use crate::process::{Process, ProcessPointer};
use crate::state::RcState;
use polling::{Event, Poller, Source};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

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
    alive: AtomicBool,
}

impl NetworkPoller {
    pub(crate) fn new() -> Self {
        NetworkPoller {
            poller: Poller::new().expect("Failed to set up the network poller"),
            alive: AtomicBool::new(true),
        }
    }

    pub(crate) fn poll(&self, events: &mut Vec<Event>) -> io::Result<bool> {
        self.poller.wait(events, None)?;
        Ok(self.is_alive())
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

    pub(crate) fn terminate(&self) {
        self.alive.store(false, Ordering::Release);
        self.poller.notify().expect("Failed to notify the poller to terminate");
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
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
    state: RcState,
}

impl Worker {
    pub(crate) fn new(state: RcState) -> Self {
        Worker { state }
    }

    pub(crate) fn run(&self) {
        let mut events = Vec::new();

        loop {
            if !self.state.network_poller.poll(&mut events).unwrap_or(false) {
                // The poller is no longer alive, so we should shut down.
                return;
            }

            for event in &events {
                let process =
                    unsafe { ProcessPointer::new(event.key as *mut Process) };

                self.state.scheduler.schedule(process);
            }

            events.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_process_class, new_process};
    use std::net::UdpSocket;

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
        poller.poll(&mut events).unwrap();

        assert!(events.capacity() >= 2);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_terminate() {
        let poller = NetworkPoller::new();
        let mut events = Vec::with_capacity(1);

        assert!(poller.is_alive());

        poller.terminate();

        assert!(!poller.poll(&mut events).unwrap());
        assert_eq!(events.capacity(), 1);
        assert_eq!(events.len(), 0);
        assert!(!poller.is_alive());
    }
}
