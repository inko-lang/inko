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
use queue::{Queue, RcQueue};
use std::collections::VecDeque;
use std::thread::{self, Builder, JoinHandle};

pub const STACK_SIZE: usize = 1024 * 1024;

pub struct Job<T: Send + 'static> {
    /// The thread ID this job is pinned to, if any.
    thread_id: Option<u8>,

    /// The value to pass to the closure that is used for consuming jobs.
    value: T,
}

impl<T: Send + 'static> Job<T> {
    pub fn new(value: T, thread_id: Option<u8>) -> Self {
        Job { thread_id, value }
    }

    /// Returns a normal job.
    pub fn normal(value: T) -> Self {
        Self::new(value, None)
    }

    /// Returns a pinned job.
    pub fn pinned(value: T, thread_id: u8) -> Self {
        Self::new(value, Some(thread_id))
    }

    /// Returns the queue index to use for this job.
    pub fn queue_index(&self) -> Option<usize> {
        self.thread_id.map(|id| id as usize)
    }
}

pub struct Worker {
    /// The unique ID of this worker in a particular pool.
    ///
    /// Workers across different pools may have the same ID.
    pub thread_id: u8,
}

impl Worker {
    pub fn new(thread_id: u8) -> Self {
        Worker { thread_id }
    }

    pub fn queue_index(&self) -> usize {
        self.thread_id as usize
    }

    /// Returns true if this worker should process the given job.
    pub fn should_process<T: Send + 'static>(&self, job: &Job<T>) -> bool {
        if let Some(job_thread_id) = job.thread_id {
            self.thread_id == job_thread_id
        } else {
            true
        }
    }
}

/// The part of a pool that is shared between threads.
pub struct PoolInner<T: Send + 'static> {
    /// The queues containing jobs to process, one for every thread.
    pub queues: Vec<RcQueue<Job<T>>>,

    /// The global queue new messages are scheduled into.
    pub global_queue: RcQueue<Job<T>>,
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
    pub fn new(amount: u8, name: Option<String>) -> Self {
        Pool {
            inner: ArcWithoutWeak::new(PoolInner::new(amount)),
            name,
        }
    }

    /// Starts a number of threads, each calling the supplied closure for every
    /// job.
    pub fn run<F>(&self, closure: F) -> JoinGuard<()>
    where
        F: Fn(&Worker, T) + Sync + Send + 'static,
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

            let result = builder.spawn(move || {
                let worker = Worker::new(idx as u8);

                inner.process(&worker, &closure)
            });

            handles.push(result.unwrap());
        }

        JoinGuard::new(handles)
    }

    /// Schedules a new job for processing.
    pub fn schedule(&self, job: Job<T>) {
        if let Some(index) = job.queue_index() {
            self.inner.queues[index].push(job);
        } else {
            self.inner.global_queue.push(job);
        }
    }

    pub fn schedule_multiple(&self, values: VecDeque<Job<T>>) {
        self.inner.global_queue.push_multiple(values);
    }

    /// Terminates all the schedulers, ignoring any remaining jobs
    pub fn terminate(&self) {
        for queue in &self.inner.queues {
            queue.terminate_queue();
        }

        self.inner.global_queue.terminate_queue();
    }
}

impl<T: Send + 'static> PoolInner<T> {
    /// Returns a new PoolInner with the given amount of queues.
    ///
    /// This method will panic if `amount` is 0.
    pub fn new(amount: u8) -> Self {
        assert!(amount > 0);

        PoolInner {
            queues: (0..amount).map(|_| Queue::with_rc()).collect(),
            global_queue: Queue::with_rc(),
        }
    }

    /// Processes jobs from a queue.
    pub fn process<F>(&self, worker: &Worker, closure: &ArcWithoutWeak<F>)
    where
        F: Fn(&Worker, T) + Sync + Send + 'static,
    {
        let queue = &self.queues[worker.queue_index()];

        while !queue.should_terminate() {
            let job = if let Some(job) = queue.pop_nonblock() {
                job
            } else if let Some(job) = self.steal_excluding(&worker) {
                job
            } else if let Some(job) = self.steal_from_global(&queue) {
                job
            } else if let Ok(job) = self.global_queue.pop() {
                job
            } else {
                break;
            };

            if worker.should_process(&job) {
                // Only process the job if it is pinned to our thread, or if it
                // is not pinned at all.
                closure(&worker, job.value);
            } else if let Some(pinned_id) = job.thread_id {
                // Pinned jobs should be pushed back into the queue of the
                // owning thread.
                self.queues[pinned_id as usize].push(job);
            }
        }
    }

    /// Steals a job from a queue.
    ///
    /// This method won't steal jobs from the queue at the given position. This
    /// allows a thread to steal jobs without checking its own queue.
    ///
    /// This method also makes sure that any stolen pinned jobs are pushed back
    /// into the appropriate queues.
    pub fn steal_excluding(&self, our_worker: &Worker) -> Option<Job<T>> {
        let ours = &self.queues[our_worker.queue_index()];

        for (index, queue) in self.queues.iter().enumerate() {
            let their_thread_id = index as u8;

            if their_thread_id != our_worker.thread_id {
                if let Some(jobs) = queue.pop_half() {
                    let (our_jobs, pinned_jobs): (
                        VecDeque<Job<T>>,
                        VecDeque<Job<T>>,
                    ) = jobs
                        .into_iter()
                        .partition(|ref job| our_worker.should_process(job));

                    // Pinned jobs need to be pushed back into the appropriate
                    // queues.
                    for job in pinned_jobs {
                        self.reschedule_job(job);
                    }

                    ours.push_multiple(our_jobs);

                    return ours.pop_nonblock();
                }
            }
        }

        None
    }

    pub fn steal_from_global(&self, ours: &RcQueue<Job<T>>) -> Option<Job<T>> {
        self.global_queue.pop_half().and_then(|jobs| {
            ours.push_multiple(jobs);
            ours.pop_nonblock()
        })
    }

    fn reschedule_job(&self, job: Job<T>) {
        if let Some(thread_id) = job.thread_id {
            self.queues[thread_id as usize].push(job);
        } else {
            self.global_queue.push(job);
        }
    }
}

impl<T> JoinGuard<T> {
    pub fn new(handles: Vec<JoinHandle<T>>) -> Self {
        JoinGuard { handles }
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
        ($condition:expr) => {{
            while $condition {
                thread::sleep(Duration::from_millis(5));
            }
        }};
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

        pool.schedule(Job::normal(1));
        pool.schedule(Job::normal(2));

        let guard = pool.run(move |_, number| {
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

        pool.schedule(Job::normal(1));
        pool.schedule(Job::normal(2));

        assert_eq!(pool.inner.global_queue.len(), 2);
    }

    #[test]
    fn test_pool_schedule_pinned() {
        let pool = Pool::new(2, None);

        pool.schedule(Job::pinned(1, 0));
        pool.schedule(Job::normal(2));

        assert_eq!(pool.inner.queues[0].len(), 1);
        assert_eq!(pool.inner.global_queue.len(), 1);
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

        let closure = ArcWithoutWeak::new(move |_: &Worker, number| {
            t_started.store(true, Ordering::Release);
            t_counter.fetch_add(number, Ordering::Relaxed);
        });

        inner.queues[0].push(Job::normal(1));

        let t_closure = closure.clone();
        let handle =
            thread::spawn(move || t_inner.process(&Worker::new(0), &t_closure));

        wait_while!(!started.load(Ordering::Acquire));

        inner.global_queue.terminate_queue();
        inner.queues[0].terminate_queue();

        handle.join().unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_pool_inner_steal_excluding() {
        let inner = PoolInner::new(2);
        let worker = Worker::new(0);

        inner.queues[1].push(Job::normal(10));
        inner.queues[1].push(Job::normal(20));
        inner.queues[1].push(Job::normal(30));

        let job = inner.steal_excluding(&worker);

        assert!(job.is_some());

        assert_eq!(job.unwrap().value, 20);
        assert_eq!(inner.queues[0].len(), 1);
        assert_eq!(inner.queues[1].len(), 1);
    }

    #[test]
    fn test_pool_inner_steal_excluding_with_pinned_jobs() {
        let inner = PoolInner::new(4);
        let worker = Worker::new(0);

        inner.queues[1].push(Job::pinned(30, 2));
        inner.queues[1].push(Job::pinned(40, 3));
        inner.queues[1].push(Job::normal(10));
        inner.queues[1].push(Job::pinned(20, 0));

        assert_eq!(inner.steal_excluding(&worker).unwrap().value, 10);

        // The pinned job 20 is now in queue 0.
        assert_eq!(inner.queues[0].len(), 1);
        assert_eq!(inner.queues[0].pop().unwrap().value, 20);

        // The remaining two jobs are left as-is, because we only steal half the
        // jobs.
        assert_eq!(inner.queues[1].len(), 2);
    }

    #[test]
    fn test_pool_inner_reschedule_job() {
        let inner = PoolInner::new(2);

        inner.reschedule_job(Job::normal(10));
        inner.reschedule_job(Job::pinned(20, 1));

        assert_eq!(inner.global_queue.pop().unwrap().value, 10);
        assert_eq!(inner.queues[1].pop().unwrap().value, 20);
    }
}
