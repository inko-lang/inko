//! Parking and waking up of multiple threads.
use parking_lot::{Condvar, Mutex};
use std::time::Duration;

macro_rules! lock_and_notify {
    ($parker: expr, $method: ident) => {
        // We need to acquire the lock, otherwise we may try to notify threads
        // between them checking their condition and unlocking the lock.
        //
        // Acquiring the lock here prevents this from happening, as we can not
        // acquire it until all threads that are about to sleep unlock the lock
        // from on their end.
        let _lock = $parker.mutex.lock();

        $parker.cvar.$method();
    };
}

/// A type for parking and waking up multiple threads easily.
///
/// A ParkGroup can be used by multiple threads to park themselves, and by other
/// threads to wake up any parked threads.
///
/// Since a ParkGroup is not associated with a single value, threads must
/// pass some sort of condition to `ParkGroup::park_while()`.
pub struct ParkGroup {
    mutex: Mutex<()>,
    cvar: Condvar,
}

impl ParkGroup {
    pub fn new() -> Self {
        ParkGroup {
            mutex: Mutex::new(()),
            cvar: Condvar::new(),
        }
    }

    /// Notifies all parked threads.
    pub fn notify_all(&self) {
        lock_and_notify!(self, notify_all);
    }

    /// Notifies a single parked thread.
    pub fn notify_one(&self) {
        lock_and_notify!(self, notify_one);
    }

    pub fn park(&self) {
        let mut lock = self.mutex.lock();

        self.cvar.wait(&mut lock);
    }

    /// Parks the current thread as long as the given condition is true.
    pub fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        let mut lock = self.mutex.lock();

        while condition() {
            self.cvar.wait(&mut lock);
        }
    }

    /// Parks the current thread as long as the given condition is true, until
    /// the timeout expires.
    ///
    /// The return value will be true if the wait timed out.
    pub fn park_while_with_timeout<F>(
        &self,
        timeout: Duration,
        condition: F,
    ) -> bool
    where
        F: Fn() -> bool,
    {
        let mut lock = self.mutex.lock();

        while condition() {
            if self.cvar.wait_for(&mut lock, timeout).timed_out() {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn test_notify_one() {
        let group = ArcWithoutWeak::new(ParkGroup::new());
        let alive = ArcWithoutWeak::new(AtomicBool::new(true));
        let started = ArcWithoutWeak::new(AtomicBool::new(true));
        let group_clone = group.clone();
        let alive_clone = alive.clone();
        let started_clone = started.clone();

        let handle = thread::spawn(move || {
            group_clone.park_while(|| {
                // We mark the thread as started here, forcing the notify_one()
                // below to wake up this thread using a condition variable;
                // instead of our condition preventing the thread from going to
                // sleep in the first place.
                started_clone.store(true, Ordering::SeqCst);

                alive_clone.load(Ordering::SeqCst)
            });

            10
        });

        while !started.load(Ordering::SeqCst) {}

        alive.store(false, Ordering::SeqCst);
        group.notify_one();

        assert_eq!(handle.join().unwrap(), 10);
    }

    #[test]
    fn test_notify_all() {
        let group = ArcWithoutWeak::new(ParkGroup::new());
        let started = ArcWithoutWeak::new(AtomicUsize::new(0));
        let alive = ArcWithoutWeak::new(AtomicBool::new(true));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let started_clone = started.clone();
            let alive_clone = alive.clone();
            let group_clone = group.clone();

            let handle = thread::spawn(move || {
                group_clone.park_while(|| {
                    started_clone.fetch_add(1, Ordering::SeqCst);

                    alive_clone.load(Ordering::SeqCst)
                });

                10
            });

            handles.push(handle);
        }

        while started.load(Ordering::SeqCst) < handles.len() {}

        alive.store(false, Ordering::SeqCst);
        group.notify_all();

        for handle in handles {
            assert_eq!(handle.join().unwrap(), 10);
        }
    }

    #[test]
    fn test_park_while_with_condition_that_is_always_false() {
        let thread = thread::spawn(|| {
            ParkGroup::new().park_while(|| false);
            10
        });

        assert_eq!(thread.join().unwrap(), 10);
    }

    #[test]
    fn test_park_while_with_timeout_with_thread() {
        let thread = thread::spawn(|| {
            ParkGroup::new()
                .park_while_with_timeout(Duration::from_millis(10), || true);
            10
        });

        assert_eq!(thread.join().unwrap(), 10);
    }

    #[test]
    fn test_park_while_with_timeout_without_thread() {
        let group = ParkGroup::new();

        assert!(
            group.park_while_with_timeout(Duration::from_millis(1), || true)
        );
    }
}
