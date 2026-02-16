use crate::network_poller::Interest;
use libc::{
    fcntl, kevent, kqueue, ENOENT, EVFILT_READ, EVFILT_WRITE, EV_ADD, EV_CLEAR,
    EV_DELETE, EV_ONESHOT, EV_RECEIPT, FD_CLOEXEC, F_SETFD,
};
use std::io::{Error, ErrorKind};
use std::mem::zeroed;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::null;

pub(crate) type Event = kevent;

pub(crate) struct Poller {
    fd: OwnedFd,
}

impl Poller {
    pub(crate) fn new() -> Poller {
        let fd = unsafe {
            let fd = kqueue();

            if fd == -1 {
                panic!("kqueue() failed: {}", Error::last_os_error());
            }

            if fcntl(fd, F_SETFD, FD_CLOEXEC) == -1 {
                panic!("fcntl() failed: {}", Error::last_os_error());
            }

            OwnedFd::from_raw_fd(fd)
        };

        Poller { fd }
    }

    pub(crate) fn poll<'a>(
        &self,
        events: &'a mut Vec<kevent>,
    ) -> impl Iterator<Item = u64> + 'a {
        loop {
            let res = unsafe {
                kevent(
                    self.fd.as_raw_fd(),
                    null(),
                    0,
                    events.as_mut_ptr(),
                    events.capacity() as _,
                    null(),
                )
            };

            if res == -1 {
                let err = Error::last_os_error();

                if let ErrorKind::Interrupted = err.kind() {
                    continue;
                } else {
                    panic!("kevent() failed: {}", err);
                }
            }

            // Safety: the above check ensures the length value is valid.
            unsafe { events.set_len(res as _) };
            return events.iter().map(|e| e.udata as u64);
        }
    }

    pub(crate) fn add(
        &self,
        id: u64,
        source: impl AsRawFd,
        interest: Interest,
    ) {
        let fd = source.as_raw_fd();
        let filter = match interest {
            Interest::Read => EVFILT_READ,
            Interest::Write => EVFILT_WRITE,
        };

        self.apply(&mut [kevent {
            ident: fd as _,
            filter,
            flags: EV_ADD | EV_CLEAR | EV_ONESHOT | EV_RECEIPT,
            udata: id as _,
            ..unsafe { zeroed() }
        }]);
    }

    pub(crate) fn modify(
        &self,
        id: u64,
        source: impl AsRawFd,
        interest: Interest,
    ) {
        let fd = source.as_raw_fd();
        let base = EV_CLEAR | EV_ONESHOT | EV_RECEIPT;
        let (read, write) = match interest {
            Interest::Read => (EV_ADD, EV_DELETE),
            Interest::Write => (EV_DELETE, EV_ADD),
        };

        self.apply(&mut [
            kevent {
                ident: fd as _,
                filter: EVFILT_READ,
                flags: base | read,
                udata: id as _,
                ..unsafe { zeroed() }
            },
            kevent {
                ident: fd as _,
                filter: EVFILT_WRITE,
                flags: base | write,
                udata: id as _,
                ..unsafe { zeroed() }
            },
        ]);
    }

    pub(crate) fn delete(&self, source: impl AsRawFd, interest: Interest) {
        let fd = source.as_raw_fd();
        let filter = match interest {
            Interest::Read => EVFILT_READ,
            Interest::Write => EVFILT_WRITE,
        };

        self.apply(&mut [kevent {
            ident: fd as _,
            filter,
            flags: EV_DELETE | EV_RECEIPT,
            ..unsafe { zeroed() }
        }]);
    }

    fn apply(&self, events: &mut [kevent]) {
        let res = unsafe {
            kevent(
                self.fd.as_raw_fd(),
                events.as_ptr(),
                events.len() as _,
                events.as_mut_ptr(),
                events.len() as _,
                null(),
            )
        };

        if res >= 0 {
            return;
        }

        let err = Error::last_os_error();

        // ENOENT may be produced when removing a non-existing event, which we
        // want to ignore.
        if err.raw_os_error() != Some(ENOENT) {
            panic!("kevent() failed: {}", err);
        }
    }
}
