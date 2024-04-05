use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use rustix::event::kqueue::{kevent, kqueue, Event, EventFilter, EventFlags};
use rustix::fd::{AsFd, AsRawFd, OwnedFd};
use rustix::io::Errno;

pub(crate) type Events = Vec<Event>;

pub(crate) struct Poller {
    fd: OwnedFd,
}

impl Poller {
    pub(crate) fn new() -> Poller {
        let fd = kqueue().expect("failed to create the kqueue descriptor");

        Poller { fd }
    }

    pub(crate) fn poll(&self, events: &mut Events) -> Vec<ProcessPointer> {
        match unsafe { kevent(&self.fd, &[], events, None) } {
            Ok(_) | Err(Errno::INTR) => {}
            Err(_) => panic!("kevent() failed"),
        }

        let procs = events
            .iter()
            .map(|e| unsafe {
                ProcessPointer::new(e.udata() as usize as *mut _)
            })
            .collect();

        events.clear();
        procs
    }

    pub(crate) fn add(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        let fd = source.as_fd().as_raw_fd();
        let (add, del) = match interest {
            Interest::Read => (EventFilter::Read(fd), EventFilter::Write(fd)),
            Interest::Write => (EventFilter::Write(fd), EventFilter::Read(fd)),
        };
        let id = process.identifier() as isize;
        let flags =
            EventFlags::CLEAR | EventFlags::ONESHOT | EventFlags::RECEIPT;
        let events = [
            Event::new(add, EventFlags::ADD | flags, id),
            Event::new(del, EventFlags::DELETE, 0),
        ];

        self.apply(&events);
    }

    pub(crate) fn modify(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        self.add(process, source, interest);
    }

    pub(crate) fn delete(&self, source: impl AsFd) {
        let fd = source.as_fd().as_raw_fd();
        let events = [
            Event::new(EventFilter::Read(fd), EventFlags::DELETE, 0),
            Event::new(EventFilter::Write(fd), EventFlags::DELETE, 0),
        ];

        self.apply(&events);
    }

    fn apply(&self, events: &[Event; 2]) {
        let mut changes = Vec::with_capacity(events.len());

        unsafe {
            match kevent(&self.fd, events, &mut changes, None) {
                Ok(_) | Err(Errno::INTR) => {}
                Err(e) => panic!("kevent() failed: {}", e),
            }
        };

        for event in changes {
            let data = event.data() as i32;

            // Per https://github.com/tokio-rs/mio/issues/582 we ignore PIPE,
            // though it's highly unlikely to ever occur in reality given the
            // affected macOS versions are quite old at this point.
            if event.flags().contains(EventFlags::ERROR)
                && data != 0
                && data != Errno::NOENT.raw_os_error()
                && data != Errno::PIPE.raw_os_error()
            {
                let err = Errno::from_raw_os_error(data);

                // In the extremely unlikely event we reach this code, there's
                // nothing we can really do but abort.
                panic!("kevent() failed to apply the changes: {}", err);
            }
        }
    }
}
