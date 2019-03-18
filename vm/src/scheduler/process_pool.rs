//! Thread pool for executing lightweight Inko processes.
use arc_without_weak::ArcWithoutWeak;
use process::RcProcess;
use scheduler::join_list::JoinList;
use scheduler::pool::Pool;
use scheduler::pool_state::PoolState;
use scheduler::process_worker::ProcessWorker;
use scheduler::queue::RcQueue;
use scheduler::worker::Worker;
use std::thread;

/// A pool of threads for running lightweight processes.
///
/// A pool consists out of one or more workers, each backed by an OS thread.
/// Workers can perform work on their own as well as steal work from other
/// workers.
pub struct ProcessPool {
    pub state: ArcWithoutWeak<PoolState<RcProcess>>,

    /// The base name of every thread in this pool.
    name: String,
}

impl ProcessPool {
    pub fn new(name: String, threads: usize) -> Self {
        assert!(
            threads > 0,
            "A ProcessPool requires at least a single thread"
        );

        Self {
            name,
            state: ArcWithoutWeak::new(PoolState::new(threads)),
        }
    }

    /// Starts the pool, blocking the current thread until the pool is
    /// terminated.
    ///
    /// The current thread will be used to perform jobs scheduled onto the first
    /// queue.
    pub fn start_main<F>(&self, callback: F) -> JoinList<()>
    where
        F: Fn(&mut ProcessWorker, RcProcess) + Send + 'static,
    {
        let rc_callback = ArcWithoutWeak::new(callback);
        let join_list = self.spawn_threads_for_range(1, &rc_callback);

        ProcessWorker::new(0, self.state.queues[0].clone(), self.state.clone())
            .run(&*rc_callback);

        join_list
    }

    /// Schedules a job onto a specific queue.
    pub fn schedule_onto_queue(&self, queue: usize, job: RcProcess) {
        self.state.schedule_onto_queue(queue, job);
    }
}

impl Pool<RcProcess, ProcessWorker> for ProcessPool {
    fn state(&self) -> &ArcWithoutWeak<PoolState<RcProcess>> {
        &self.state
    }

    fn spawn_thread<F>(
        &self,
        id: usize,
        queue: RcQueue<RcProcess>,
        callback: ArcWithoutWeak<F>,
    ) -> thread::JoinHandle<()>
    where
        F: Fn(&mut ProcessWorker, RcProcess) + Send + 'static,
    {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("{} {}", self.name, id))
            .spawn(move || {
                ProcessWorker::new(id, queue, state).run(&*callback);
            })
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use vm::test::setup;

    #[test]
    #[should_panic]
    fn test_new_with_zero_threads() {
        ProcessPool::new("test".to_string(), 0);
    }

    #[test]
    fn test_start_main() {
        let pool = ProcessPool::new("test".to_string(), 1);
        let (_machine, _block, process) = setup();
        let pid = ArcWithoutWeak::new(Mutex::new(10));
        let pid_copy = pid.clone();

        pool.schedule(process);

        let threads = pool.start_main(move |worker, process| {
            *pid_copy.lock() = process.pid;
            worker.state().terminate();
        });

        threads.join().unwrap();

        assert_eq!(*pid.lock(), 0);
    }

    #[test]
    fn test_schedule_onto_queue() {
        let pool = ProcessPool::new("test".to_string(), 1);
        let (_machine, _block, process) = setup();

        pool.schedule_onto_queue(0, process);

        assert!(pool.state.queues[0].has_external_jobs());
    }

    #[test]
    fn test_spawn_thread() {
        let pool = ProcessPool::new("test".to_string(), 1);
        let (_machine, _block, process) = setup();
        let pid = ArcWithoutWeak::new(Mutex::new(10));
        let pid_copy = pid.clone();

        let callback = ArcWithoutWeak::new(
            move |worker: &mut ProcessWorker, process: RcProcess| {
                *pid_copy.lock() = process.pid;
                worker.state().terminate();
            },
        );

        let thread =
            pool.spawn_thread(0, pool.state.queues[0].clone(), callback);

        pool.schedule(process);

        thread.join().unwrap();

        assert_eq!(*pid.lock(), 0);
    }
}
