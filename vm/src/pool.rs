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
//!     let pool = Pool::new(4, None);
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
//!     let pool = Pool::new(4, None);
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

use arc_without_weak::ArcWithoutWeak;
use std::collections::VecDeque;
use std::thread::{self, Builder, JoinHandle};

use queue::{Queue, RcQueue};

pub const STACK_SIZE: usize = 1024 * 1024;

/// The part of a pool that is shared between threads.
pub struct PoolInner<T: Send + 'static> {
    /// The queues containing jobs to process, one for every thread.
    pub queues: Vec<RcQueue<T>>,

    /// The global queue new messages are scheduled into.
    pub global_queue: RcQueue<T>,
}

/// A pool of threads, each processing jobs of a given type.
pub struct Pool<T: Send + 'static> {
    /// The part of a pool that is shared between scheduler threads.
    pub inner: ArcWithoutWeak<PoolInner<T>>,

    /// The name of this pool, if any.
    pub name: Option<String>,
}

/// A RAII guard wrapping multiple JoinHandles.
pub struct JoinGuard<T> {
    handles: Vec<JoinHandle<T>>,
}

impl<T: Send + 'static> Pool<T> {
    /// Returns a new Pool with the given amount of queues.
    pub fn new(amount: usize, name: Option<String>) -> Self {
        Pool {
            inner: ArcWithoutWeak::new(PoolInner::new(amount)),
            name: name,
        }
    }

    /// Starts a number of threads, each calling the supplied closure for every
    /// job.
    pub fn run<F>(&self, closure: F) -> JoinGuard<()>
    where
        F: Fn(T) + Sync + Send + 'static,
    {
        let arc_closure = ArcWithoutWeak::new(closure);
        let amount = self.inner.queues.len();
        let mut handles = Vec::with_capacity(amount);

        for idx in 0..amount {
            let inner = self.inner.clone();
            let closure = arc_closure.clone();
            let mut builder = Builder::new().stack_size(STACK_SIZE);

            if let Some(name) = self.name.as_ref() {
                builder = builder.name(format!("{} {}", name, idx));
            }

            let result = builder.spawn(move || inner.process(idx, closure));

            handles.push(result.unwrap());
        }

        JoinGuard::new(handles)
    }

    /// Schedules a new job for processing.
    pub fn schedule(&self, value: T) {
        self.inner.global_queue.push(value);
    }

    pub fn schedule_multiple(&self, values: VecDeque<T>) {
        self.inner.global_queue.push_multiple(values);
    }

    /// Terminates all the schedulers, ignoring any remaining jobs
    pub fn terminate(&self) {
        for queue in self.inner.queues.iter() {
            queue.terminate_queue();
        }

        self.inner.global_queue.terminate_queue();
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
            global_queue: Queue::with_rc(),
        }
    }

    /// Processes jobs from a queue.
    pub fn process<F>(&self, index: usize, closure: ArcWithoutWeak<F>)
    where
        F: Fn(T) + Sync + Send + 'static,
    {
        let ref queue = self.queues[index];

        while !queue.should_terminate() {
            let job = if let Some(job) = queue.pop_nonblock() {
                job
            } else if let Some(job) = self.steal_excluding(index) {
                job
            } else if let Some(job) = self.steal_from_global(&queue) {
                job
            } else {
                if let Ok(job) = self.global_queue.pop() {
                    job
                } else {
                    break;
                }
            };

            closure(job);
        }
    }

    /// Steals a job from a queue.
    ///
    /// This method won't steal jobs from the queue at the given position. This
    /// allows a thread to steal jobs without checking its own queue.
    pub fn steal_excluding(&self, excluding: usize) -> Option<T> {
        let ours = &self.queues[excluding];

        for (index, queue) in self.queues.iter().enumerate() {
            if index != excluding {
                if let Some(jobs) = queue.pop_half() {
                    ours.push_multiple(jobs);

                    return ours.pop_nonblock();
                }
            }
        }

        None
    }

    pub fn steal_from_global(&self, ours: &RcQueue<T>) -> Option<T> {
        self.global_queue.pop_half().and_then(|jobs| {
            ours.push_multiple(jobs);
            ours.pop_nonblock()
        })
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    macro_rules! wait_while {
        ($condition: expr) => ({
            while $condition {
                thread::sleep(Duration::from_millis(5));
            }
        });
    }

    #[test]
    fn test_pool_new() {
        let pool: Pool<()> = Pool::new(2, None);

        assert_eq!(pool.inner.queues.len(), 2);
    }

    #[test]
    fn test_pool_run() {
        let pool = Pool::new(2, None);
        let counter = ArcWithoutWeak::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        pool.schedule(1);
        pool.schedule(2);

        let guard = pool.run(move |number| {
            counter_clone.fetch_add(number, Ordering::Relaxed);
        });

        // Wait until all jobs have been completed.
        wait_while!(pool.inner.global_queue.len() > 0);

        pool.terminate();

        guard.join().unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_pool_schedule() {
        let pool = Pool::new(2, None);

        pool.schedule(1);
        pool.schedule(1);

        assert_eq!(pool.inner.global_queue.len(), 2);
    }

    #[test]
    fn test_pool_terminate() {
        let pool: Pool<()> = Pool::new(2, None);

        pool.terminate();

        assert!(pool.inner.queues[0].should_terminate());
        assert!(pool.inner.queues[1].should_terminate());
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
        let inner = ArcWithoutWeak::new(PoolInner::new(1));
        let counter = ArcWithoutWeak::new(AtomicUsize::new(0));
        let started = ArcWithoutWeak::new(AtomicBool::new(false));

        let t_inner = inner.clone();
        let t_counter = counter.clone();
        let t_started = started.clone();

        let closure = ArcWithoutWeak::new(move |number| {
            t_started.store(true, Ordering::Release);
            t_counter.fetch_add(number, Ordering::Relaxed);
        });

        inner.queues[0].push(1);

        let t_closure = closure.clone();
        let handle = thread::spawn(move || t_inner.process(0, t_closure));

        wait_while!(!started.load(Ordering::Acquire));

        inner.global_queue.terminate_queue();
        inner.queues[0].terminate_queue();

        handle.join().unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_inner_steal_excluding() {
        let inner = PoolInner::new(2);

        inner.queues[1].push(10);
        inner.queues[1].push(20);
        inner.queues[1].push(30);

        let job = inner.steal_excluding(0);

        assert!(job.is_some());

        assert_eq!(job.unwrap(), 20);
        assert_eq!(inner.queues[0].len(), 1);
        assert_eq!(inner.queues[1].len(), 1);
    }
}
