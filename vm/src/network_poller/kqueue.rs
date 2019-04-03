//! Polling of non-blocking sockets for BSD based systems.
//!
//! This module provides support for polling non-blocking sockets on BSD based
//! systems using kqueue.
use super::event_id::EventId;
use super::interest::Interest;
use super::unix::map_error;
use nix::errno::Errno;
use nix::sys::event::{
    kevent_ts, kqueue, EventFilter, EventFlag, FilterFlag, KEvent,
};
use nix::unistd::close;
use std::io;
use std::mem;
use std::ops::Drop;
use std::os::unix::io::{AsRawFd, RawFd};

macro_rules! kevent {
    ($fd:expr, $filter:ident, $flags:expr, $id:expr) => {
        KEvent::new(
            $fd.as_raw_fd() as usize,
            EventFilter::$filter,
            $flags,
            FilterFlag::empty(),
            0,
            $id.value() as isize,
        )
    };
}

/// A collection of kqueue events.
pub struct Events {
    events: Vec<KEvent>,
}

#[cfg_attr(feature = "cargo-clippy", allow(len_without_is_empty))]
impl Events {
    pub fn with_capacity(amount: usize) -> Self {
        Events {
            events: Vec::with_capacity(amount),
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    pub fn event_ids<'a>(&'a self) -> impl Iterator<Item = EventId> + 'a {
        self.events.iter().map(|e| EventId(e.udata() as u64))
    }

    fn set_len(&mut self, amount: usize) {
        unsafe {
            self.events.set_len(amount);
        }
    }
}

/// Polling of non-blocking sockets using kqueue.
pub struct NetworkPoller {
    fd: RawFd,
}

unsafe impl Sync for NetworkPoller {}
unsafe impl Send for NetworkPoller {}

impl NetworkPoller {
    pub fn new() -> NetworkPoller {
        let fd = kqueue().expect("Failed to create a kqueue file descriptor");

        NetworkPoller { fd }
    }

    pub fn poll(&self, events: &mut Events) -> io::Result<()> {
        // The nix crate uses slice lengths, but our buffer is a Vec. To make
        // this work we have to manually set the length of our Vec.
        //
        // We don't need to clear the input buffer, as old events will either be
        // overwritten or deallocated when the Events structure is dropped.
        events.set_len(events.events.capacity());

        let received =
            map_error(kevent_ts(self.fd, &[], &mut events.events, None))?;

        // The number of events might be smaller than the desired length, so we
        // need to readjust the length of our buffer.
        events.set_len(received);

        Ok(())
    }

    pub fn register<T: AsRawFd>(
        &self,
        fd: &T,
        id: EventId,
        interest: Interest,
    ) -> io::Result<()> {
        let flags =
            EventFlag::EV_CLEAR | EventFlag::EV_ONESHOT | EventFlag::EV_RECEIPT;

        // Reads and writes are registered as separate events. This means that
        // if we want a read, we have to make sure previous writes are disabled.
        let (read_flag, write_flag) = match interest {
            Interest::Read => (EventFlag::EV_ADD, EventFlag::EV_DELETE),
            Interest::Write => (EventFlag::EV_DELETE, EventFlag::EV_ADD),
        };

        let changes = [
            kevent!(fd, EVFILT_READ, flags | read_flag, id),
            kevent!(fd, EVFILT_WRITE, flags | write_flag, id),
        ];

        let mut changed: [KEvent; 2] = unsafe { mem::uninitialized() };

        map_error(kevent_ts(self.fd, &changes, &mut changed, None))?;

        for event in &changed {
            if event.data() == 0 {
                continue;
            }

            let errno = Errno::from_i32(event.data() as i32);

            // When adding an event of one type (e.g. read), we'll attempt to
            // remove the other (a write), but that event might not exist. If
            // this happens an ENOENT is produced, and we'll ignore it.
            if event.flags().contains(EventFlag::EV_DELETE)
                && errno == Errno::ENOENT
            {
                continue;
            }

            return Err(io::Error::from(errno));
        }

        Ok(())
    }

    pub fn reregister<T: AsRawFd>(
        &self,
        fd: &T,
        id: EventId,
        interest: Interest,
    ) -> io::Result<()> {
        // Re-adding an existing event will modify it, so we can just use
        // register().
        self.register(fd, id, interest)
    }
}

impl Drop for NetworkPoller {
    fn drop(&mut self) {
        close(self.fd).expect("Failed to close the kqueue file descriptor");
    }
}
