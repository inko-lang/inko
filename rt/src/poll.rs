use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use crate::state::State;
use std::os::fd::{BorrowedFd, RawFd};

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
    pub registered: i8,
}

impl Poll {
    /// Registers `self` with a network poller.
    ///
    /// This must be done when the run lock for the given process is acquired,
    /// otherwise we may end up rescheduling a process multiple times or observe
    /// an inconsistent state between threads.
    pub(crate) unsafe fn register(
        &mut self,
        state: &State,
        process: ProcessPointer,
        interest: Interest,
    ) {
        // Safety: the standard library guarantees the file descriptor is valid
        // at this point.
        let fd = unsafe { BorrowedFd::borrow_raw(self.inner) };
        let cur = self.registered;

        if cur == NOT_REGISTERED {
            let new = self.inner as usize % state.network_pollers.len();

            self.registered = new as i8;
            state.network_pollers[new].add(process, fd, interest);
        } else {
            state.network_pollers[cur as usize].modify(process, fd, interest);
        }
    }

    pub(crate) unsafe fn deregister(
        &mut self,
        state: &State,
        interest: Interest,
    ) {
        // Safety: the standard library guarantees the file descriptor is valid
        // at this point.
        let fd = unsafe { BorrowedFd::borrow_raw(self.inner) };

        state.network_pollers[self.registered as usize].delete(fd, interest);
        self.registered = NOT_REGISTERED;
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
