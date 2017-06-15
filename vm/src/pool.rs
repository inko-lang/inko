//! Performing work in a pool of threads.
//!
//! A Pool can be used to perform a set of jobs using a pool of threads. Work is
//! distributed evenly upon scheduling, and threads may steal work from other
//! threads.
//!
//! ## Scheduling
//!
//! Scheduling is done using a round-robin approach. Whenever a new job is
//! scheduled it's scheduled on the queue with the least amount of jobs
//! available.
//!
//! This is somewhat different from a usual work stealing queue where work is
//! scheduled on the current thread's queue instead. This is because threads
//! suspend themselves when no work is available. Combining this with the usual
//! scheduling approach could lead to multiple threads being suspended and never
//! waking up again.
//!
//! It's possible for multiple threads to schedule jobs at the same time,
//! opposed to the usual setup where only a single producer is allowed.
//!
//! Scheduling jobs is done using `Pool::schedule()`:
//!
//!     let pool = Pool::new(4);
//!
//!     pool.schedule(4);
//!     pool.schedule(8);
//!
//!     let guard = pool.run(move |number| number * 2);
//!
//!     guard.join().unwrap();
//!
//! ## Work Stealing
//!
//! Threads may steal jobs from other threads. Stealing jobs is only done when a
//! thread has no work to do, and only regular jobs are stolen (e.g. termination
//! jobs are not stolen).
//!
//! ## Job Order
//!
//! The order in which jobs are processed is arbitrary and should not be relied
//! upon.
//!
//! ## Suspending Threads
//!
//! A thread will suspend itself if it has no work to perform and it could not
//! steal jobs from any other threads. A thread is woken up again whenever a new
//! job is scheduled in its queue.
//!
//! ## Shutting Down
//!
//! Pools are active until they are explicitly shut down. Shutting down a pool
//! can be done by calling `Pool::terminate()`. For example:
//!
//!     let pool = Pool::new(4);
//!     let guard = pool.run(move |job| do_something(job));
//!
//!     pool.schedule(10);
//!     pool.schedule(20);
//!
//!     // Terminate the pool
//!     pool.terminate();
//!
//!     // Wait for threads to terminate
//!     guard.join().unwrap();

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use queue::{Queue, RcQueue};

/// A job to be processed by a thread.
pub enum Job<T: Send + 'static> {
    /// A thread should terminate itself.
    Terminate,

    /// A thread should perform the given job
    Perform(T),
}

impl<T: Send + 'static> Job<T> {
    pub fn can_steal(&self) -> bool {
        match self {
            &Job::Perform(_) => true,
            _ => false,
        }
    }
}

/// The part of a pool that is shared between threads.
pub struct PoolInner<T: Send + 'static> {
    /// The queues containing jobs to process, one for every thread.
    pub queues: Vec<RcQueue<Job<T>>>,

    /// Boolean used by schedulers to determine if they should pull jobs from
    /// the queues.
    pub process: AtomicBool,
}

/// A pool of threads, each processing jobs of a given type.
pub struct Pool<T: Send + 'static> {
    /// The part of a pool that is shared between scheduler threads.
    pub inner: Arc<PoolInner<T>>,
}

/// A RAII guard wrapping multiple JoinHandles.
pub struct JoinGuard<T> {
    handles: Vec<JoinHandle<T>>,
}

impl<T: Send + 'static> Pool<T> {
    /// Returns a new Pool with the given amount of queues.
    pub fn new(amount: usize) -> Self {
        Pool { inner: Arc::new(PoolInner::new(amount)) }
    }

    /// Starts a number of threads, each calling the supplied closure for every
    /// job.
    pub fn run<F>(&self, closure: F) -> JoinGuard<()>
    where
        F: Fn(T) + Sync + Send + 'static,
    {
        let arc_closure = Arc::new(closure);
        let amount = self.inner.queues.len();
        let mut handles = Vec::with_capacity(amount);

        for idx in 0..amount {
            let inner = self.inner.clone();
            let closure = arc_closure.clone();

            handles.push(thread::spawn(move || inner.process(idx, closure)));
        }

        JoinGuard::new(handles)
    }

    /// Schedules a new job for processing.
    pub fn schedule(&self, value: T) {
        let mut queue_index = 0;
        let mut queue_size = self.inner.queues[0].len();

        for (index, queue) in self.inner.queues.iter().enumerate() {
            let current_size = queue.len();

            if current_size < queue_size {
                queue_index = index;
                queue_size = current_size;
            }
        }

        self.inner.queues[queue_index].push(Job::Perform(value));
    }

    /// Terminates all the schedulers, ignoring any remaining jobs
    pub fn terminate(&self) {
        self.inner.stop_processing();
        self.schedule_termination();
    }

    fn schedule_termination(&self) {
        for queue in self.inner.queues.iter() {
            queue.push(Job::Terminate);
        }
    }
}

impl<T: Send + 'static> PoolInner<T> {
    /// Returns a new PoolInner with the given amount of queues.
    ///
    /// This method will panic if `amount` is 0.
    pub fn new(amount: usize) -> Self {
        assert!(amount > 0);

        PoolInner {
            queues: (0..amount).map(|_| Queue::with_rc()).collect(),
            process: AtomicBool::new(true),
        }
    }

    /// Processes jobs from a queue.
    pub fn process<F>(&self, index: usize, closure: Arc<F>)
    where
        F: Fn(T) + Sync + Send + 'static,
    {
        let ref queue = self.queues[index];

        while self.should_process() {
            let job = queue.pop_nonblock().unwrap_or_else(|| {
                self.steal_excluding(index).unwrap_or_else(|| queue.pop())
            });

            match job {
                Job::Terminate => break,
                Job::Perform(value) => closure(value),
            };
        }
    }

    /// Steals a job from a queue.
    ///
    /// This method won't steal jobs from the queue at the given position. This
    /// allows a thread to steal jobs without checking its own queue.
    pub fn steal_excluding(&self, excluding: usize) -> Option<Job<T>> {
        for (index, queue) in self.queues.iter().enumerate() {
            if index != excluding {
                if let Some(job) = queue.pop_nonblock() {
                    if job.can_steal() {
                        return Some(job);
                    } else {
                        queue.push(job);
                    }
                }
            }
        }

        None
    }

    pub fn should_process(&self) -> bool {
        self.process.load(Ordering::Relaxed)
    }

    pub fn stop_processing(&self) {
        self.process.store(false, Ordering::Relaxed);
    }
}

impl<T> JoinGuard<T> {
    pub fn new(handles: Vec<JoinHandle<T>>) -> Self {
        JoinGuard { handles: handles }
    }

    /// Waits for all the threads to finish.
    pub fn join(self) -> thread::Result<()> {
        for handle in self.handles {
            handle.join()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn test_pool_new() {
        let pool: Pool<()> = Pool::new(2);

        assert_eq!(pool.inner.queues.len(), 2);
    }

    #[test]
    fn test_pool_run() {
        let pool = Pool::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        pool.schedule(1);
        pool.schedule(2);

        let guard = pool.run(move |number| {
            counter_clone.fetch_add(number, Ordering::Relaxed);
        });

        // We're not using "terminate" here to ensure all jobs are processed.
        pool.schedule_termination();

        guard.join().unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_pool_schedule_balancing_jobs() {
        let pool = Pool::new(2);

        pool.schedule(1);
        pool.schedule(1);

        assert_eq!(pool.inner.queues[0].len(), 1);
        assert_eq!(pool.inner.queues[1].len(), 1);
    }

    #[test]
    fn test_pool_terminate() {
        let pool: Pool<()> = Pool::new(2);

        pool.terminate();

        assert_eq!(pool.inner.queues[0].len(), 1);
        assert_eq!(pool.inner.queues[1].len(), 1);

        assert!(match pool.inner.queues[0].pop() {
            Job::Terminate => true,
            _ => false,
        });

        assert!(match pool.inner.queues[1].pop() {
            Job::Terminate => true,
            _ => false,
        });

        assert_eq!(pool.inner.should_process(), false);
    }

    #[test]
    fn test_pool_inner_new() {
        let inner: PoolInner<()> = PoolInner::new(2);

        assert_eq!(inner.queues.len(), 2);
    }

    #[test]
    #[should_panic]
    fn test_pool_inner_new_invalid_amount() {
        PoolInner::<()>::new(0);
    }

    #[test]
    fn test_pool_inner_process() {
        let inner = Arc::new(PoolInner::new(1));
        let counter = Arc::new(AtomicUsize::new(0));

        let t_inner = inner.clone();
        let t_counter = counter.clone();

        let closure = Arc::new(move |number| {
            t_counter.fetch_add(number, Ordering::Relaxed);
        });

        let t_closure = closure.clone();
        let handle = thread::spawn(move || t_inner.process(0, t_closure));

        inner.queues[0].push(Job::Perform(1));
        inner.queues[0].push(Job::Terminate);

        handle.join().unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_inner_steal_excluding() {
        let inner = PoolInner::new(2);

        inner.queues[0].push(Job::Perform(10));
        inner.queues[0].push(Job::Perform(20));
        inner.queues[1].push(Job::Perform(30));

        let job = inner.steal_excluding(0);

        assert!(job.is_some());

        match job.unwrap() {
            Job::Terminate => panic!("expected Perform, got Terminate"),
            Job::Perform(val) => assert_eq!(val, 30),
        }

        assert_eq!(inner.queues[0].len(), 2);
        assert_eq!(inner.queues[1].len(), 0);
    }

    #[test]
    fn test_pool_inner_should_process() {
        let inner: PoolInner<()> = PoolInner::new(1);

        assert!(inner.should_process());
    }

    #[test]
    fn test_pool_inner_stop_processing() {
        let inner: PoolInner<()> = PoolInner::new(1);

        inner.stop_processing();

        assert_eq!(inner.should_process(), false);
    }
}
