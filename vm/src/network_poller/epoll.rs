//! Polling of non-blocking sockets for Linux.
//!
//! This module provides support for polling non-blocking sockets on Linux using
//! epoll.
use super::event_id::EventId;
use super::interest::Interest;
use super::unix::map_error;
use nix::sys::epoll::{
    epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp,
};
use nix::unistd::close;
use std::io;
use std::ops::Drop;
use std::os::unix::io::{AsRawFd, RawFd};

const POLL_INDEFINITELY: isize = -1;

/// A collection of epoll events.
pub struct Events {
    events: Vec<EpollEvent>,
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
        self.events.iter().map(|e| EventId(e.data()))
    }

    fn set_len(&mut self, amount: usize) {
        unsafe {
            self.events.set_len(amount);
        }
    }
}

/// Polling of non-blocking sockets using epoll.
pub struct NetworkPoller {
    fd: RawFd,
}

unsafe impl Sync for NetworkPoller {}
unsafe impl Send for NetworkPoller {}

impl NetworkPoller {
    pub fn new() -> NetworkPoller {
        let fd =
            epoll_create().expect("Failed to create an epoll file descriptor");

        NetworkPoller { fd }
    }

    pub fn poll(&self, events: &mut Events) -> io::Result<()> {
        // The nix crate uses slice lengths, but our buffer is a Vec. To make
        // this work we have to manually set the length of our Vec.
        //
        // We don't need to clear the input buffer, as old events will either be
        // overwritten or deallocated when the Events structure is dropped.
        events.set_len(events.events.capacity());

        let received = map_error(epoll_wait(
            self.fd,
            &mut events.events,
            POLL_INDEFINITELY,
        ))?;

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
        self.register_or_reregister(fd, id, interest, false)
    }

    pub fn reregister<T: AsRawFd>(
        &self,
        fd: &T,
        id: EventId,
        interest: Interest,
    ) -> io::Result<()> {
        self.register_or_reregister(fd, id, interest, true)
    }

    fn register_or_reregister<T: AsRawFd>(
        &self,
        fd: &T,
        id: EventId,
        interest: Interest,
        update: bool,
    ) -> io::Result<()> {
        let mut flags = match interest {
            Interest::Read => EpollFlags::EPOLLIN,
            Interest::Write => EpollFlags::EPOLLOUT,
        };

        // We always want edge triggered events so we don't wake up too many
        // threads at once, and oneshot events so we don't keep producing events
        // while rescheduling a process.
        flags = flags | EpollFlags::EPOLLET | EpollFlags::EPOLLONESHOT;

        let mut event = EpollEvent::new(flags, id.value());

        let operation = if update {
            EpollOp::EpollCtlMod
        } else {
            EpollOp::EpollCtlAdd
        };

        map_error(epoll_ctl(
            self.fd,
            operation,
            fd.as_raw_fd(),
            Some(&mut event),
        ))?;

        Ok(())
    }
}

impl Drop for NetworkPoller {
    fn drop(&mut self) {
        close(self.fd).expect("Failed to close the epoll file descriptor");
    }
}
