//! Efficient work stealing queues.
use crate::arc_without_weak::ArcWithoutWeak;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_deque::{Steal, Stealer, Worker};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A Queue can be used to store, pop, and steal jobs to perform. Threads that
/// own a Queue can push jobs into the queue with minimal overhead, while other
/// threads can push jobs into the queue using a Multiple Producer Multiple
/// Consumer (MPMC) channel.
pub(crate) struct Queue<T: Send> {
    /// The worker side of the deque, used for producing new jobs. This
    /// structure _can only_ be used by the thread that owns this queue.
    worker: Worker<T>,

    /// The stealing side of the deque. Other workers may still jobs from this
    /// queue using this stealer.
    stealer: Stealer<T>,

    /// The number of pending jobs that were scheduled externally.
    pending_external: AtomicUsize,

    /// The Sender to be used by other threads that wish to schedule jobs onto
    /// this queue.
    sender: Sender<T>,

    /// The receiver to use for receiving jobs scheduled by other threads. This
    /// Receiver _can only_ be used by the thread that owns the queue.
    receiver: Receiver<T>,
}

pub(crate) type RcQueue<T> = ArcWithoutWeak<Queue<T>>;

impl<T: Send> Queue<T> {
    pub(crate) fn new() -> Self {
        let worker = Worker::new_fifo();
        let stealer = worker.stealer();
        let (sender, receiver) = unbounded();

        Queue {
            stealer,
            pending_external: AtomicUsize::new(0),
            worker,
            sender,
            receiver,
        }
    }

    pub(crate) fn with_rc() -> ArcWithoutWeak<Queue<T>> {
        ArcWithoutWeak::new(Self::new())
    }

    pub(crate) fn pending_external(&self) -> usize {
        self.pending_external.load(Ordering::Acquire)
    }

    pub(crate) fn increment_pending_external(&self) {
        self.pending_external.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn decrement_pending_external(&self) {
        if self.pending_external() > 0 {
            self.pending_external.fetch_sub(1, Ordering::Release);
        }
    }

    /// Pushes a job onto the deque.
    ///
    /// This method can only be used by the thread that owns the queue.
    pub(crate) fn push_internal(&self, value: T) {
        self.worker.push(value);
    }

    /// Pushes a job onto the shared channel.
    ///
    /// This method can be safely used by multiple threads.
    pub(crate) fn push_external(&self, value: T) {
        self.increment_pending_external();

        self.sender
            .send(value)
            .expect("Attempted to schedule a job onto a queue that is dropped");
    }

    /// Pops a value from the worker.
    pub(crate) fn pop(&self) -> Option<T> {
        self.worker.pop()
    }

    /// Steal one or more jobs and push them into the given queue.
    ///
    /// This method can safely be used by different threads. The returned
    /// boolean will be `true` if one or more jobs were stolen, `false`
    /// otherwise.
    pub(crate) fn steal_into(&self, queue: &Self) -> bool {
        loop {
            match self.stealer.steal_batch(&queue.worker) {
                Steal::Empty => return false,
                Steal::Success(_) => return true,
                _ => {}
            };
        }
    }

    /// Pops a job from the public channel, without first moving it to the
    /// private Worker.
    pub(crate) fn pop_external_job(&self) -> Option<T> {
        let job = self.receiver.try_recv().ok();

        if job.is_some() {
            self.decrement_pending_external();
        }

        job
    }

    /// Moves all jobs from the public channel into the private Worker, without
    /// blocking the calling thread.
    ///
    /// This method will return `true` if one or more jobs were moved into the
    /// local queue.
    pub(crate) fn move_external_jobs(&self) -> bool {
        // We only receive up to the number of currently pending jobs. If many
        // jobs are scheduled rapidly, simply receiving until we run out of
        // messages could result in this method taking a very long time to
        // return.
        let remaining = self.pending_external();

        if remaining == 0 {
            return false;
        }

        let mut received = 0;

        for job in self.receiver.try_iter().take(remaining) {
            received += 1;

            self.worker.push(job);
        }

        self.pending_external.fetch_sub(received, Ordering::Release);

        received > 0
    }

    /// Returns true if there are one or more jobs stored in the external queue.
    pub(crate) fn has_external_jobs(&self) -> bool {
        self.pending_external() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_internal() {
        let queue = Queue::new();

        queue.push_internal(10);

        assert_eq!(queue.worker.is_empty(), false);
    }

    #[test]
    fn test_push_external() {
        let queue = Queue::new();

        queue.push_external(10);

        assert_eq!(queue.pending_external(), 1);
        assert_eq!(queue.receiver.try_iter().count(), 1);
    }

    #[test]
    fn test_pop() {
        let queue = Queue::new();

        assert!(queue.pop().is_none());

        queue.push_internal(10);

        assert_eq!(queue.pop(), Some(10));
    }

    #[test]
    fn test_steal() {
        let queue1 = Queue::new();
        let queue2 = Queue::new();

        assert_eq!(queue1.steal_into(&queue2), false);

        queue1.push_internal(10);

        assert!(queue1.steal_into(&queue2));
        assert!(queue1.worker.is_empty());
        assert_eq!(queue2.pop(), Some(10));
    }

    #[test]
    fn test_move_external_jobs() {
        let queue = Queue::new();

        queue.push_external(10);
        queue.push_external(20);
        queue.push_external(30);

        assert_eq!(queue.pending_external(), 3);
        assert!(queue.move_external_jobs());
        assert_eq!(queue.pending_external(), 0);
        assert_eq!(queue.pop(), Some(10));
        assert_eq!(queue.pop(), Some(20));
        assert_eq!(queue.pop(), Some(30));
    }

    #[test]
    fn test_move_external_with_limited_number_of_jobs() {
        let queue = Queue::new();

        for i in 0..8 {
            queue.push_external(i);
        }

        assert!(queue.move_external_jobs());
        assert_eq!(queue.pending_external(), 0);
        assert_eq!(queue.receiver.try_iter().count(), 0);
    }

    #[test]
    fn test_move_external_jobs_without_jobs() {
        let queue: Queue<()> = Queue::new();

        assert_eq!(queue.move_external_jobs(), false);
    }

    #[test]
    fn test_has_local_jobs() {
        let queue = Queue::new();

        assert!(queue.worker.is_empty());

        queue.push_internal(10);

        assert!(!queue.worker.is_empty());
    }

    #[test]
    fn test_pop_external_job() {
        let queue = Queue::new();

        assert!(queue.pop_external_job().is_none());

        queue.push_external(10);

        assert_eq!(queue.pop_external_job(), Some(10));
        assert_eq!(queue.pending_external(), 0);
    }

    #[test]
    fn test_has_external_jobs() {
        let queue = Queue::new();

        assert_eq!(queue.has_external_jobs(), false);

        queue.push_external(10);

        assert!(queue.has_external_jobs());
    }
}
