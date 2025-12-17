use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::state::State;
use std::os::fd::{BorrowedFd, RawFd};
use std::sync::atomic::{AtomicI8, Ordering};

/// The registered value to use to signal a source isn't registered with a
/// network poller.
const NOT_REGISTERED: i8 = -1;

/// A type that can be polled using epoll/kqueue/etc.
///
/// When changing the layout of this type, don't forget to also update its
/// definition in the standard library.
#[repr(C)]
pub struct Poll {
    /// The file descriptor of the source.
    ///
    /// This is a raw file descriptor as the standard library is in charge of
    /// dropping/closing it.
    pub inner: RawFd,

    /// The ID of the network poller we're registered with.
    ///
    /// A value of -1 indicates the source isn't registered with any poller.
    ///
    /// This flag is necessary because the system's polling mechanism may not
    /// allow overwriting existing registrations without setting some additional
    /// flags. For example, epoll requires the use of EPOLL_CTL_MOD when
    /// overwriting a registration, as using EPOLL_CTL_ADD will produce an error
    /// if a file descriptor is already registered.
    pub registered: AtomicI8,
}

impl Poll {
    pub(crate) fn register(
        &mut self,
        state: &State,
        process: ProcessPointer,
        interest: Interest,
    ) {
        let cur = self.registered.load(Ordering::Acquire);

        // Safety: the standard library guarantees the file descriptor is valid
        // at this point.
        let fd = unsafe { BorrowedFd::borrow_raw(self.inner) };

        // Once registered, the process might be rescheduled immediately if
        // there is data available. This means that once we (re)register the
        // source, it is not safe to use "self" anymore.
        //
        // To deal with this we:
        //
        // 1. Set "registered" _first_ (if necessary)
        // 2. Add the source to the poller
        if cur == NOT_REGISTERED {
            let new = self.inner as usize % state.network_pollers.len();

            self.registered.store(new as i8, Ordering::Release);
            state.network_pollers[new].add(process, fd, interest);
        } else {
            state.network_pollers[cur as usize].modify(process, fd, interest);
        }
        // *DO NOT* use "self" from here on, as the source/process may already
        // be running on a different thread.
    }

    pub(crate) fn deregister(&mut self, state: &State) {
        let poller_id = self.registered.load(Ordering::Acquire) as usize;

        // Safety: the standard library guarantees the file descriptor is valid
        // at this point.
        let fd = unsafe { BorrowedFd::borrow_raw(self.inner) };

        state.network_pollers[poller_id].delete(fd);
        self.registered.store(NOT_REGISTERED, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_type_size() {
        assert_eq!(size_of::<Poll>(), 8);
    }
}
