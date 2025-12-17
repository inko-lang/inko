use crate::network_poller::Interest;
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

    pub(crate) fn poll<'a>(
        &self,
        events: &'a mut Events,
    ) -> impl Iterator<Item = u64> + 'a {
        match unsafe { kevent(&self.fd, &[], events, None) } {
            Ok(_) | Err(Errno::INTR) => {}
            Err(_) => panic!("kevent() failed"),
        }

        events.iter().map(|e| e.udata() as u64)
    }

    pub(crate) fn add(&self, id: u64, source: impl AsFd, interest: Interest) {
        let id = id as isize;
        let fd = source.as_fd().as_raw_fd();
        let flags =
            EventFlags::CLEAR | EventFlags::ONESHOT | EventFlags::RECEIPT;
        let events = match interest {
            Interest::Read => [
                Event::new(EventFilter::Read(fd), EventFlags::ADD | flags, id),
                Event::new(EventFilter::Write(fd), EventFlags::DELETE, 0),
            ],
            Interest::Write => [
                Event::new(EventFilter::Write(fd), EventFlags::ADD | flags, id),
                Event::new(EventFilter::Read(fd), EventFlags::DELETE, 0),
            ],
        };

        self.apply(&events);
    }

    pub(crate) fn modify(
        &self,
        id: u64,
        source: impl AsFd,
        interest: Interest,
    ) {
        self.add(id, source, interest);
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
