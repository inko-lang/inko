//! Broadcasting values to one or more threads.
use parking_lot::{Condvar, Mutex};

struct Inner<T: Clone> {
    /// The value to receive.
    value: Option<T>,

    /// A boolean indicating the broadcast channel is shut down.
    active: bool,

    /// The maximum number of times a value can be received, before it's reset.
    pending: usize,
}

/// A channel that  can be used to broadcast a single value across threads.
/// Values can be loaded a limited number of times before they are cleared.
pub struct Broadcast<T: Clone> {
    inner: Mutex<Inner<T>>,
    cvar: Condvar,
}

impl<T: Clone> Broadcast<T> {
    pub fn new() -> Self {
        Broadcast {
            inner: Mutex::new(Inner {
                value: None,
                active: true,
                pending: 0,
            }),
            cvar: Condvar::new(),
        }
    }

    /// Shuts down the broadcast channel.
    pub fn shutdown(&self) {
        let mut lock = self.inner.lock();

        lock.active = false;

        drop(lock);
        self.cvar.notify_all();
    }

    /// Sends a value to the receivers, allowing up to `limit` receivers to
    /// receive the value.
    ///
    /// Sending a new value will overwrite the previous one.
    ///
    /// The `limit` argument specifies the maximum number of times the value can
    /// be received.
    pub fn send(&self, limit: usize, value: T) {
        let mut lock = self.inner.lock();

        lock.value = Some(value);
        lock.pending = limit;

        drop(lock);
        self.cvar.notify_all();
    }

    /// Receives a value from the channel.
    ///
    /// This method blocks until either a value is received, or the channel is
    /// shut down.
    pub fn recv(&self) -> Option<T> {
        loop {
            let mut lock = self.inner.lock();

            if !lock.active {
                return None;
            }

            let value = if lock.pending == 1 {
                lock.value.take()
            } else {
                lock.value.clone()
            };

            if lock.pending > 0 {
                lock.pending -= 1;
            }

            if let Some(value) = value {
                return Some(value);
            } else {
                self.cvar.wait(&mut lock);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_receive() {
        let channel = Broadcast::new();

        channel.send(2, 42);

        assert_eq!(channel.recv(), Some(42));
        assert_eq!(channel.recv(), Some(42));

        let inner = channel.inner.lock();

        assert!(inner.value.is_none());
        assert_eq!(inner.pending, 0);
    }

    #[test]
    fn test_shutdown() {
        let channel = Broadcast::new();

        channel.send(2, 42);
        channel.shutdown();

        assert!(channel.recv().is_none());
    }
}
