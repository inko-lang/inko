//! State management of a thread pool.
use crate::scheduler::park_group::ParkGroup;
use crate::scheduler::queue::{Queue, RcQueue};
use crossbeam_deque::{Injector, Steal};
use std::iter;
use std::sync::atomic::{AtomicBool, Ordering};

/// The maximum number of threads a single pool allows.
const MAX_THREADS: usize = 255;

/// The internal state of a single pool, shared between the many workers that
/// belong to the pool.
pub struct PoolState<T: Send> {
    /// The queues available for workers to store work in and steal work from.
    pub queues: Vec<RcQueue<T>>,

    /// A boolean indicating if the scheduler is alive, or should shut down.
    alive: AtomicBool,

    /// The global queue on which new jobs will be scheduled,
    global_queue: Injector<T>,

    /// Used for parking and unparking worker threads.
    park_group: ParkGroup,
}

impl<T: Send> PoolState<T> {
    /// Creates a new state for the given number worker threads.
    ///
    /// Threads are not started by this method, and instead must be started
    /// manually.
    pub fn new(mut threads: usize) -> Self {
        if threads > MAX_THREADS {
            threads = MAX_THREADS;
        }

        let queues = iter::repeat_with(Queue::with_rc).take(threads).collect();

        PoolState {
            alive: AtomicBool::new(true),
            queues,
            global_queue: Injector::new(),
            park_group: ParkGroup::new(),
        }
    }

    /// Schedules a new job onto the global queue.
    pub fn push_global(&self, value: T) {
        self.global_queue.push(value);
        self.park_group.notify_one();
    }

    /// Schedules a job onto a specific queue.
    ///
    /// This method will panic if the queue index is invalid.
    pub fn schedule_onto_queue(&self, queue: usize, value: T) {
        self.queues[queue].push_external(value);

        // A worker might be parked when sending it an external message, so we
        // have to wake them up. We have to notify all workers instead of a
        // single one, otherwise we may end up notifying a different worker.
        self.park_group.notify_all();
    }

    /// Pops a value off the global queue.
    ///
    /// This method will block the calling thread until a value is available.
    pub fn pop_global(&self) -> Option<T> {
        loop {
            match self.global_queue.steal() {
                Steal::Empty => {
                    return None;
                }
                Steal::Retry => {}
                Steal::Success(value) => {
                    return Some(value);
                }
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn terminate(&self) {
        self.alive.store(false, Ordering::Release);
        self.park_group.notify_all();
    }

    /// Parks the current thread as long as the given condition is true.
    pub fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        self.park_group
            .park_while(|| self.is_alive() && condition());
    }

    /// Returns true if one or more jobs are present in the global queue.
    pub fn has_global_jobs(&self) -> bool {
        !self.global_queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use std::sync::Barrier;
    use std::thread;

    #[test]
    fn test_new() {
        let state: PoolState<()> = PoolState::new(4);

        assert_eq!(state.queues.len(), 4);
    }

    #[test]
    fn test_new_with_too_many_threads() {
        let state: PoolState<()> = PoolState::new(MAX_THREADS + 1);

        assert_eq!(state.queues.len(), MAX_THREADS);
    }

    #[test]
    fn test_push_global() {
        let state = PoolState::new(1);

        state.push_global(10);

        assert_eq!(state.global_queue.is_empty(), false);
    }

    #[test]
    fn test_pop_global() {
        let state = PoolState::new(1);

        state.push_global(10);

        assert_eq!(state.pop_global(), Some(10));
        assert_eq!(state.pop_global(), None);
    }

    #[test]
    fn test_terminate() {
        let state: PoolState<()> = PoolState::new(4);

        assert!(state.is_alive());

        state.terminate();

        assert_eq!(state.is_alive(), false);
    }

    #[test]
    fn test_park_while() {
        let state: PoolState<()> = PoolState::new(4);
        let mut number = 0;

        state.park_while(|| false);

        number += 1;

        state.terminate();
        state.park_while(|| true);

        number += 1;

        assert_eq!(number, 2);
    }

    #[test]
    fn test_has_global_jobs() {
        let state = PoolState::new(4);

        assert_eq!(state.has_global_jobs(), false);

        state.push_global(10);

        assert!(state.has_global_jobs());
    }

    #[test]
    fn test_schedule_onto_queue() {
        let state = PoolState::new(1);

        state.schedule_onto_queue(0, 10);

        assert!(state.queues[0].has_external_jobs());
    }

    #[test]
    #[should_panic]
    fn test_schedule_onto_invalid_queue() {
        let state = PoolState::new(1);

        state.schedule_onto_queue(1, 10);
    }

    #[test]
    fn test_schedule_onto_queue_wake_up() {
        let state = ArcWithoutWeak::new(PoolState::new(1));
        let state_clone = state.clone();
        let barrier = ArcWithoutWeak::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let handle = thread::spawn(move || {
            let queue = &state_clone.queues[0];

            barrier_clone.wait();

            state_clone.park_while(|| !queue.has_external_jobs());

            queue.pop_external_job().unwrap()
        });

        // This test is always racy, as we can not guarantee the below schedule
        // runs after the thread has gone to sleep. For example, it might run
        // _just_ before the above thread parks itself. Using `thread::sleep()`
        // whould slow down tests, and there's no guarantee the sleep time would
        // always be enough.
        barrier.wait();

        state.schedule_onto_queue(0, 10);

        assert_eq!(handle.join().unwrap(), 10);
    }
}
