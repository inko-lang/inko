use crate::network_poller::Interest;
use libc::{
    epoll_create, epoll_ctl, epoll_event, epoll_wait, EPOLLET, EPOLLIN,
    EPOLLONESHOT, EPOLLOUT, EPOLL_CLOEXEC, EPOLL_CTL_ADD, EPOLL_CTL_DEL,
    EPOLL_CTL_MOD,
};
use std::ffi::c_int;
use std::io::Error;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::null_mut;

fn flags_for(interest: Interest) -> u32 {
    let flags = match interest {
        Interest::Read => EPOLLIN,
        Interest::Write => EPOLLOUT,
    };

    (flags | EPOLLET | EPOLLONESHOT) as u32
}

pub(crate) type Event = epoll_event;

pub(crate) struct Poller {
    fd: OwnedFd,
}

impl Poller {
    pub(crate) fn new() -> Poller {
        let fd = unsafe { epoll_create(EPOLL_CLOEXEC) };

        if fd == -1 {
            panic!("epoll_create() failed");
        }

        // Safety: we checked the file descriptor so at this point it's
        // guaranteed to be valid.
        Poller { fd: unsafe { OwnedFd::from_raw_fd(fd) } }
    }

    pub(crate) fn poll<'a>(
        &self,
        events: &'a mut Vec<Event>,
    ) -> impl Iterator<Item = u64> + 'a {
        let res = unsafe {
            epoll_wait(
                self.fd.as_raw_fd(),
                events.as_mut_ptr(),
                events.capacity() as _,
                -1,
            )
        };

        if res == -1 {
            panic!("epoll_wait() failed: {}", Error::last_os_error());
        }

        // Safety: the above check ensures the length value is valid.
        unsafe { events.set_len(res as _) };
        events.iter().map(|e| e.u64)
    }

    pub(crate) fn add(
        &self,
        id: u64,
        source: impl AsRawFd,
        interest: Interest,
    ) {
        let mut event = epoll_event { events: flags_for(interest), u64: id };

        self.ctl(EPOLL_CTL_ADD, source, Some(&mut event));
    }

    pub(crate) fn modify(
        &self,
        id: u64,
        source: impl AsRawFd,
        interest: Interest,
    ) {
        let mut event = epoll_event { events: flags_for(interest), u64: id };

        self.ctl(EPOLL_CTL_MOD, source, Some(&mut event));
    }

    pub(crate) fn delete(&self, source: impl AsRawFd, _interest: Interest) {
        self.ctl(EPOLL_CTL_DEL, source, None);
    }

    fn ctl(&self, op: c_int, fd: impl AsRawFd, event: Option<&mut Event>) {
        let res = unsafe {
            epoll_ctl(
                self.fd.as_raw_fd(),
                op,
                fd.as_raw_fd(),
                event.map(|v| v as *mut _).unwrap_or_else(null_mut),
            )
        };

        if res == -1 {
            panic!("epoll_ctl() failed: {}", Error::last_os_error());
        }
    }
}
