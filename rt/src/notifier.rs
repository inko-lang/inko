use atomic_wait::{wait, wake_all, wake_one};
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct Token(u32);

/// A lock-free condition variable using the platform's lightweight mutex
/// implementation (e.g. futex on Linux).
pub(crate) struct Notifier {
    value: AtomicU32,
}

impl Notifier {
    pub(crate) fn new() -> Self {
        Self { value: AtomicU32::new(0) }
    }

    pub(crate) fn notify_one(&self) {
        self.value.fetch_add(1, Ordering::AcqRel);
        wake_one(&self.value);
    }

    pub(crate) fn notify_all(&self) {
        self.value.fetch_add(1, Ordering::AcqRel);
        wake_all(&self.value);
    }

    pub(crate) fn prepare_wait(&self) -> Token {
        Token(self.value.load(Ordering::Acquire))
    }

    pub(crate) fn wait(&self, token: Token) {
        wait(&self.value, token.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::thread::scope;

    #[test]
    fn test_notify_one() {
        let not = Notifier::new();
        let barrier = Barrier::new(2);
        let res = scope(|s| {
            let waiter = s.spawn(|| {
                let tok = not.prepare_wait();

                barrier.wait();
                not.wait(tok);
                true
            });

            barrier.wait();
            not.notify_one();
            waiter.join().unwrap()
        });

        assert!(res);
    }

    #[test]
    fn test_notify_all() {
        let not = Notifier::new();
        let barrier = Barrier::new(3);
        let res = scope(|s| {
            let w1 = s.spawn(|| {
                let tok = not.prepare_wait();

                barrier.wait();
                not.wait(tok);
                true
            });

            let w2 = s.spawn(|| {
                let tok = not.prepare_wait();

                barrier.wait();
                not.wait(tok);
                true
            });

            barrier.wait();
            not.notify_all();
            (w1.join().unwrap(), w2.join().unwrap())
        });

        assert_eq!(res, (true, true));
    }

    #[test]
    fn test_prepare_wait() {
        let not = Notifier::new();
        let tok = not.prepare_wait();

        assert_eq!(tok.0, 0);
        assert_eq!(not.value.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_commit_wait_with_changed_value() {
        let not = Notifier::new();
        let tok = not.prepare_wait();

        not.notify_one();
        not.wait(tok);

        // There's no state/value to assert here. Instead, if commit_wait()
        // isn't implemented properly this test hangs forever.
    }
}
