//! An unbounded, synchronized queue
//!
//! A Queue can be used as an unbounded, synchronized queue. This can be useful
//! when you want to share data using multiple producers and consumers but only
//! want a single consumer (without specifically selecting which one) to obtain
//! a value in the queue.
//!
//! Values are processed in FIFO order.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar};

pub struct Queue<T> {
    values: Mutex<VecDeque<T>>,
    signaler: Condvar,
}

pub type RcQueue<T> = Arc<Queue<T>>;

impl<T> Queue<T> {
    /// Returns a new Queue.
    pub fn new() -> Self {
        Queue {
            values: Mutex::new(VecDeque::new()),
            signaler: Condvar::new(),
        }
    }

    /// Returns a new queue that can be shared between threads.
    pub fn with_rc() -> Arc<Self> {
        Arc::new(Self::new())
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
        let mut values = unlock!(self.values);

        values.push_back(value);

        // We don't need _all_ listeners to wait up as chances are only one may
        // get a value, thus we only wake up one of them.
        self.signaler.notify_one();
    }

    /// Removes the first value from the queue and returns it.
    ///
    /// If no values are available in the queue this method will block until at
    /// least a single value is available.
    ///
    /// # Examples
    ///
    ///     let queue = Queue::new();
    ///
    ///     queue.push(10);
    ///     queue.pop();
    pub fn pop(&self) -> T {
        if let Some(value) = unlock!(self.values).pop_front() {
            return value;
        }

        let mut values = unlock!(self.values);

        while values.len() == 0 {
            values = self.signaler.wait(values).unwrap();
        }

        values.pop_front().unwrap()
    }

    /// Removes the first value from the queue without blocking the caller if
    /// there are no values in the queue.
    pub fn pop_nonblock(&self) -> Option<T> {
        unlock!(self.values).pop_front()
    }

    /// Pops all messages off the queue.
    pub fn pop_all(&self) -> VecDeque<T> {
        let mut values = unlock!(self.values);
        let mut popped = VecDeque::with_capacity(values.len());

        for pointer in values.drain(0..) {
            popped.push_back(pointer);
        }

        popped
    }

    /// Returns the amount of values in the queue.
    pub fn len(&self) -> usize {
        unlock!(self.values).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

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
        queue.pop();

        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_pop_blocking_single_consumer() {
        let queue = Queue::with_rc();
        let queue_clone = queue.clone();
        let handle = thread::spawn(move || queue_clone.pop());

        queue.push(10);

        assert_eq!(handle.join().unwrap(), 10);
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
