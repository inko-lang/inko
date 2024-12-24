//! Polling of non-blocking sockets using the system's polling mechanism.
use crate::process::RescheduleRights;
use crate::state::RcState;

#[cfg(target_os = "linux")]
mod epoll;

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
mod kqueue;

#[cfg(target_os = "linux")]
use crate::network_poller::epoll as sys;

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
use crate::network_poller::kqueue as sys;

/// The maximum number of events to poll in a single call.
///
/// We deliberately use a large capacity here in order to reduce the amount of
/// poll wakeups, improving performance when many sockets become available at
/// the same time.
const CAPACITY: usize = 1024;

/// A poller for non-blocking sockets.
pub(crate) type NetworkPoller = sys::Poller;

/// The type of event a poller should wait for.
#[derive(Debug)]
pub(crate) enum Interest {
    Read,
    Write,
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

    pub(crate) fn run(&mut self) {
        let mut events = sys::Events::with_capacity(CAPACITY);
        let poller = &self.state.network_pollers[self.id];

        loop {
            let mut processes = poller.poll(&mut events);

            processes.retain(|proc| {
                let mut state = proc.state();
                let rights = state.try_reschedule_for_io();

                // A process may have also been registered with the timeout
                // thread (e.g. when using a timeout). As such we should only
                // reschedule the process if the timout thread didn't already do
                // this for us.
                match rights {
                    RescheduleRights::Failed => false,
                    RescheduleRights::Acquired => true,
                    RescheduleRights::AcquiredWithTimeout => {
                        self.state.timeout_worker.increase_expired_timeouts();
                        true
                    }
                }
            });

            self.state.scheduler.schedule_multiple(processes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{empty_process_type, new_process};
    use std::net::UdpSocket;

    #[test]
    fn test_add() {
        let typ = empty_process_type("A");
        let process = new_process(*typ);
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        poller.add(*process, &output, Interest::Read);
    }

    #[test]
    fn test_modify() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type("A");
        let process = new_process(*typ);

        poller.add(*process, &output, Interest::Read);
        poller.modify(*process, &output, Interest::Write);
    }

    #[test]
    fn test_delete() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type("A");
        let process = new_process(*typ);

        poller.add(*process, &output, Interest::Write);
        poller.delete(&output);
    }

    #[test]
    fn test_poll() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type("A");
        let process = new_process(*typ);
        let mut events = sys::Events::with_capacity(1);

        poller.add(*process, &output, Interest::Write);
        let procs = poller.poll(&mut events);

        assert_eq!(procs.len(), 1);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_poll_with_lower_capacity() {
        let sock1 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let sock2 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type("A");
        let proc1 = new_process(*typ);
        let proc2 = new_process(*typ);
        let mut events = sys::Events::with_capacity(1);

        poller.add(*proc1, &sock1, Interest::Write);
        poller.add(*proc2, &sock2, Interest::Write);

        let procs = poller.poll(&mut events);

        assert_eq!(procs.len(), 1);
        assert_eq!(events.len(), 0);

        let procs = poller.poll(&mut events);

        assert_eq!(procs.len(), 1);
        assert_eq!(events.len(), 0);
    }
}
