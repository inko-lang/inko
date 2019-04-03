//! Polling of non-blocking sockets using the system's polling mechanism.
pub mod event_id;
pub mod interest;
pub mod worker;

#[cfg(unix)]
pub mod unix;

#[cfg(target_os = "linux")]
pub mod epoll;

#[cfg(any(
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly",
    target_os = "macos"
))]
pub mod kqueue;

#[cfg(windows)]
pub mod wepoll;

#[cfg(target_os = "linux")]
use crate::network_poller::epoll as sys;

#[cfg(any(
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly",
    target_os = "macos"
))]
use crate::network_poller::kqueue as sys;

#[cfg(windows)]
use crate::network_poller::wepoll as sys;

/// A type for polling non-blocking sockets.
pub type NetworkPoller = sys::NetworkPoller;

/// A collection of events produced by a `NetworkPoller`.
pub type Events = sys::Events;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_poller::event_id::EventId;
    use crate::network_poller::interest::Interest;
    use std::net::UdpSocket;

    #[test]
    fn test_register() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        assert!(poller.register(&output, EventId(0), Interest::Read).is_ok());
    }

    #[test]
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn test_reregister_invalid() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        assert!(poller
            .reregister(&output, EventId(0), Interest::Read)
            .is_ok());
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn test_reregister_invalid() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        assert!(poller
            .reregister(&output, EventId(0), Interest::Read)
            .is_err());
    }

    #[test]
    fn test_reregister_valid() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();

        assert!(poller.register(&output, EventId(0), Interest::Read).is_ok());

        assert!(poller
            .reregister(&output, EventId(0), Interest::Write)
            .is_ok());
    }

    #[test]
    fn test_poll() {
        let output = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let mut events = Events::with_capacity(1);

        poller
            .register(&output, EventId(1), Interest::Write)
            .unwrap();

        assert!(poller.poll(&mut events).is_ok());

        assert_eq!(events.capacity(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events.event_ids().next().unwrap(), EventId(1));
    }

    #[test]
    fn test_poll_with_lower_capacity() {
        let sock1 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let sock2 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let mut events = Events::with_capacity(1);

        poller
            .register(&sock1, EventId(0), Interest::Write)
            .unwrap();

        poller
            .register(&sock2, EventId(1), Interest::Write)
            .unwrap();

        poller.poll(&mut events).unwrap();

        assert_eq!(events.capacity(), 1);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_poll_with_enough_capacity() {
        let sock1 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let sock2 = UdpSocket::bind("0.0.0.0:0").unwrap();
        let poller = NetworkPoller::new();
        let mut events = Events::with_capacity(2);

        poller
            .register(&sock1, EventId(0), Interest::Write)
            .unwrap();

        poller
            .register(&sock2, EventId(1), Interest::Write)
            .unwrap();

        poller.poll(&mut events).unwrap();

        assert_eq!(events.capacity(), 2);
        assert_eq!(events.len(), 2);

        let event_ids = events.event_ids().collect::<Vec<_>>();

        assert!(event_ids.contains(&EventId(0)));
        assert!(event_ids.contains(&EventId(1)));
    }
}
