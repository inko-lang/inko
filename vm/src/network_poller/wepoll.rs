//! Polling of non-blocking sockets for Windows.
//!
//! This module provides support for polling non-blocking sockets on Windows
//! using the wepoll ()https://github.com/piscisaureus/wepoll) library.
use super::event_id::EventId;
use super::interest::Interest;
use std::io;
use std::os::windows::io::AsRawSocket;
use wepoll_binding::{Epoll, EventFlag, Events as WepollEvents};

/// A collection of wepoll events.
pub struct Events {
    events: WepollEvents,
}

#[cfg_attr(feature = "cargo-clippy", allow(len_without_is_empty))]
impl Events {
    pub fn with_capacity(amount: usize) -> Self {
        Events {
            events: WepollEvents::with_capacity(amount),
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
}

/// Polling of non-blocking sockets using wepoll.
pub struct NetworkPoller {
    epoll: Epoll,
}

impl NetworkPoller {
    pub fn new() -> NetworkPoller {
        let epoll =
            Epoll::new().expect("Failed to create the wepoll file descriptor");

        NetworkPoller { epoll }
    }

    pub fn poll(&self, events: &mut Events) -> io::Result<()> {
        self.epoll.poll(&mut events.events, None)?;

        Ok(())
    }

    pub fn register<T: AsRawSocket>(
        &self,
        socket: &T,
        id: EventId,
        interest: Interest,
    ) -> io::Result<()> {
        self.epoll
            .register(socket, self.flags_for(interest), id.value())
    }

    pub fn reregister<T: AsRawSocket>(
        &self,
        socket: &T,
        id: EventId,
        interest: Interest,
    ) -> io::Result<()> {
        self.epoll
            .reregister(socket, self.flags_for(interest), id.value())
    }

    fn flags_for(&self, interest: Interest) -> EventFlag {
        let flags = match interest {
            Interest::Read => EventFlag::IN,
            Interest::Write => EventFlag::OUT,
        };

        flags | EventFlag::ONESHOT
    }
}
