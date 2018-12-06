//! An unbounded, synchronized queue
//!
//! A Queue can be used as an unbounded, synchronized queue. This can be useful
//! when you want to share data using multiple producers and consumers but only
//! want a single consumer (without specifically selecting which one) to obtain
//! a value in the queue.
//!
//! Values are processed in FIFO order.
use arc_without_weak::ArcWithoutWeak;
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;

struct Inner<T> {
    values: VecDeque<T>,
    terminate: bool,
}

pub struct Queue<T> {
    inner: Mutex<Inner<T>>,
    signaler: Condvar,
}

pub type RcQueue<T> = ArcWithoutWeak<Queue<T>>;

#[cfg_attr(feature = "cargo-clippy", allow(len_without_is_empty))]
impl<T> Queue<T> {
    /// Returns a new Queue.
    pub fn new() -> Self {
        Queue {
            inner: Mutex::new(Inner {
                values: VecDeque::new(),
                terminate: false,
            }),
            signaler: Condvar::new(),
        }
    }

    /// Returns a new queue that can be shared between threads.
    pub fn with_rc() -> ArcWithoutWeak<Self> {
        ArcWithoutWeak::new(Self::new())
    }

    pub fn terminate_queue(&self) {
        self.inner.lock().terminate = true;
        self.signaler.notify_all();
    }

    pub fn should_terminate(&self) -> bool {
        self.inner.lock().terminate
    }

    /// Pushes a value to the end of the queue.
    ///
    /// # Examples
    ///
    ///     let queue = Queue::new();
    ///
    ///     queue.push(10);
    ///     queue.push(20);
    pub fn push(&self, value: T) {
        self.inner.lock().values.push_back(value);

        // We don't need _all_ listeners to wait up as chances are only one may
        // get a value, thus we only wake up one of them.
        self.signaler.notify_one();
    }

    /// Removes the first value from the queue and returns it.
    ///
    /// If no values are available in the queue this method will block until at
    /// least a single value is available.
    ///
    /// This method will return an Err when we're in a blocking call while the
    /// queue is being terminated.
    pub fn pop(&self) -> Result<T, ()> {
        if let Some(value) = self.inner.lock().values.pop_front() {
            return Ok(value);
        }

        let mut inner = self.inner.lock();

        while inner.values.is_empty() {
            if inner.terminate {
                return Err(());
            }

            self.signaler.wait(&mut inner);
        }

        Ok(inner.values.pop_front().unwrap())
    }

    /// Pops half the amount of messages from this queue.
    pub fn pop_half(&self) -> Option<VecDeque<T>> {
        let mut inner = self.inner.lock();
        let index = inner.values.len() / 2;

        if inner.values.is_empty() {
            None
        } else {
            Some(inner.values.split_off(index))
        }
    }

    /// Pushes multiple values into the queue.
    pub fn push_multiple(&self, mut to_add: VecDeque<T>) {
        self.inner.lock().values.append(&mut to_add);
    }

    /// Removes the first value from the queue without blocking the caller if
    /// there are no values in the queue.
    pub fn pop_nonblock(&self) -> Option<T> {
        self.inner.lock().values.pop_front()
    }

    /// Pops all messages off the queue.
    pub fn pop_all(&self) -> VecDeque<T> {
        let mut inner = self.inner.lock();
        let drain = inner.values.drain(0..);

        drain.collect()
    }

    /// Returns the amount of values in the queue.
    pub fn len(&self) -> usize {
        self.inner.lock().values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    macro_rules! wait_while {
        ($condition:expr) => {{
            while $condition {
                thread::sleep(Duration::from_millis(5));
            }
        }};
    }

    #[test]
    fn test_push() {
        let queue = Queue::new();

        queue.push(10);

        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_pop() {
        let queue = Queue::new();

        queue.push(10);

        assert!(queue.pop().is_ok());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_pop_half_empty() {
        let queue: Queue<()> = Queue::new();

        assert!(queue.pop_half().is_none());
    }

    #[test]
    fn test_pop_half_with_items() {
        let queue = Queue::new();

        queue.push(10);
        queue.push(20);

        let popped_opt = queue.pop_half();

        assert!(popped_opt.is_some());

        let mut popped = popped_opt.unwrap();

        assert_eq!(popped.len(), 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(popped.pop_front().unwrap(), 20);
    }

    #[test]
    fn test_push_multiple() {
        let queue = Queue::new();
        let mut items = VecDeque::new();

        items.push_back(10);
        items.push_back(20);

        queue.push_multiple(items);

        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_pop_blocking_single_consumer() {
        let queue = Queue::with_rc();
        let queue_clone = queue.clone();
        let handle = thread::spawn(move || queue_clone.pop());

        queue.push(10);

        let popped = handle.join().unwrap();

        assert!(popped.is_ok());
        assert_eq!(popped.unwrap(), 10);
    }

    #[test]
    fn test_pop_terminating_queue() {
        let queue: RcQueue<()> = Queue::with_rc();
        let started = ArcWithoutWeak::new(AtomicBool::new(false));

        let queue_clone = queue.clone();
        let started_clone = started.clone();

        let handle = thread::spawn(move || {
            started_clone.store(true, Ordering::Release);
            queue_clone.pop()
        });

        // Briefly spin until the thread has started.
        wait_while!(!started.load(Ordering::Acquire));

        queue.terminate_queue();

        assert!(handle.join().unwrap().is_err());
    }

    #[test]
    fn test_pop_nonblock() {
        let queue: Queue<()> = Queue::new();

        assert!(queue.pop_nonblock().is_none());
    }

    #[test]
    fn test_pop_all() {
        let queue = Queue::new();

        queue.push(10);
        queue.push(20);

        let popped = queue.pop_all();

        assert_eq!(popped.len(), 2);
        assert_eq!(popped[0], 10);
        assert_eq!(popped[1], 20);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_len() {
        let queue = Queue::new();

        assert_eq!(queue.len(), 0);

        queue.push(10);

        assert_eq!(queue.len(), 1);
    }
}
