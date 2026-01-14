//! Polling of non-blocking sockets using the system's polling mechanism.
use crate::process::{ProcessPointer, RescheduleRights};
use crate::state::RcState;
use std::os::fd::{AsFd, AsRawFd};

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

/// A mask to apply to unset the lower two bits.
///
/// These bits may be used when registering multiple events for the same
/// process. In this case the bits are moved into the process' state.
const EVENT_MASK: u64 = 0b0011;

/// The type of event a poller should wait for.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Interest {
    Read,
    Write,
}

/// A poller for non-blocking sockets.
pub(crate) struct NetworkPoller {
    inner: sys::Poller,
}

impl NetworkPoller {
    pub(crate) fn new() -> Self {
        Self { inner: sys::Poller::new() }
    }

    pub(crate) fn poll<'a>(
        &self,
        events: &'a mut Vec<sys::Event>,
    ) -> impl Iterator<Item = ProcessPointer> + 'a {
        events.clear();

        self.inner.poll(events).map(|id| unsafe {
            match id & EVENT_MASK {
                0 => ProcessPointer::new(id as _),
                v => {
                    let proc = ProcessPointer::new((id & !EVENT_MASK) as _);

                    proc.state().set_poll_bit(v as u8);
                    proc
                }
            }
        })
    }

    pub(crate) fn add(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        let fd = source.as_fd().as_raw_fd();

        self.inner.add(process.identifier() as _, fd, interest);
    }

    pub(crate) fn modify(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        let fd = source.as_fd().as_raw_fd();

        self.inner.modify(process.identifier() as _, fd, interest)
    }

    pub(crate) fn delete(&self, source: impl AsFd, interest: Interest) {
        let fd = source.as_fd().as_raw_fd();

        self.inner.delete(fd, interest);
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

    pub(crate) fn run(&mut self) {
        let mut events = Vec::with_capacity(CAPACITY);
        let mut procs = Vec::with_capacity(128);
        let poller = &self.state.network_pollers[self.id];

        loop {
            for proc in poller.poll(&mut events) {
                // Acquiring the rights first _then_ matching on then ensures we
                // don't deadlock with the timeout worker.
                let rights = proc.state().try_reschedule_for_io();

                // A process may have also been registered with the timeout
                // thread (e.g. when using a timeout). As such we should only
                // reschedule the process if the timeout thread didn't already
                // do this for us.
                match rights {
                    RescheduleRights::Failed => {}
                    RescheduleRights::Acquired => procs.push(proc),
                    RescheduleRights::AcquiredWithTimeout(id) => {
                        self.state.timeout_worker.expire(id);
                        procs.push(proc);
                    }
                }
            }

            self.state.scheduler.schedule_multiple(&mut procs);
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
        let typ = empty_process_type();
        let process = new_process(typ.as_pointer());
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        poller.add(*process, &output, Interest::Read);
    }

    #[test]
    fn test_modify() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type();
        let process = new_process(typ.as_pointer());

        poller.add(*process, &output, Interest::Read);
        poller.modify(*process, &output, Interest::Write);
    }

    #[test]
    fn test_delete() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type();
        let process = new_process(typ.as_pointer());

        poller.add(*process, &output, Interest::Write);
        poller.delete(&output, Interest::Write);
    }

    #[test]
    fn test_poll() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type();
        let process = new_process(typ.as_pointer());
        let mut events = Vec::with_capacity(1);

        poller.add(*process, &output, Interest::Write);

        let procs = poller.poll(&mut events).collect::<Vec<_>>();

        assert_eq!(procs.len(), 1);
    }

    #[test]
    fn test_poll_with_bits() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type();
        let process = new_process(typ.as_pointer());
        let mut events = Vec::with_capacity(1);

        let tagged =
            unsafe { ProcessPointer::new((process.identifier() | 0b01) as _) };
        poller.add(tagged, &output, Interest::Write);

        let procs = poller.poll(&mut events).collect::<Vec<_>>();

        assert_eq!(procs, vec![*process]);
        assert_eq!(process.check_timeout_and_take_poll_bits(), (false, 0b01));
    }

    #[test]
    fn test_poll_with_lower_capacity() {
        let sock1 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let sock2 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let typ = empty_process_type();
        let proc1 = new_process(typ.as_pointer());
        let proc2 = new_process(typ.as_pointer());
        let mut events = Vec::with_capacity(1);

        poller.add(*proc1, &sock1, Interest::Write);
        poller.add(*proc2, &sock2, Interest::Write);

        let procs = poller.poll(&mut events).collect::<Vec<_>>();

        assert_eq!(procs.len(), 1);

        let procs = poller.poll(&mut events).collect::<Vec<_>>();

        assert_eq!(procs.len(), 1);
    }
}
