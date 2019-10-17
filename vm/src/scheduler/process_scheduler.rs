//! Scheduling and execution of lightweight Inko processes.
use crate::process::RcProcess;
use crate::scheduler::process_pool::ProcessPool;

/// The ID of the queue that is processed by the main thread.
const MAIN_THREAD_QUEUE_ID: usize = 0;

/// A ProcessScheduler handles the execution of processes.
///
/// A ProcessScheduler consists out of two pools: a primary pool, and a blocking
/// pool. The primary pool is used for executing all processes by default.
/// Processes may be moved to the blocking pool (and back) whenever they need to
/// perform a blocking operation, such as reading from a file.
pub struct ProcessScheduler {
    /// The pool to use for executing most processes.
    pub primary_pool: ProcessPool,

    /// The pool to use for executing processes that perform blocking
    /// operations.
    pub blocking_pool: ProcessPool,
}

impl ProcessScheduler {
    /// Creates a new ProcessScheduler with the given number of primary and
    /// blocking threads.
    pub fn new(primary: usize, blocking: usize) -> Self {
        ProcessScheduler {
            primary_pool: ProcessPool::new("primary".to_string(), primary),
            blocking_pool: ProcessPool::new("blocking".to_string(), blocking),
        }
    }

    /// Informs the scheduler it needs to terminate as soon as possible.
    pub fn terminate(&self) {
        self.primary_pool.terminate();
        self.blocking_pool.terminate();
    }

    /// Schedules a process in one of the pools.
    pub fn schedule(&self, process: RcProcess) {
        let pool = if process.is_blocking() {
            &self.blocking_pool
        } else {
            &self.primary_pool
        };

        if let Some(thread_id) = process.thread_id() {
            pool.schedule_onto_queue(thread_id as usize, process);
        } else {
            pool.schedule(process);
        }
    }

    /// Schedules a process onto the main thread.
    pub fn schedule_on_main_thread(&self, process: RcProcess) {
        self.primary_pool
            .schedule_onto_queue(MAIN_THREAD_QUEUE_ID, process);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::test::setup;

    #[test]
    fn test_terminate() {
        let scheduler = ProcessScheduler::new(1, 1);

        scheduler.terminate();

        assert_eq!(scheduler.primary_pool.state.is_alive(), false);
        assert_eq!(scheduler.blocking_pool.state.is_alive(), false);
    }

    #[test]
    fn test_schedule_on_primary() {
        let scheduler = ProcessScheduler::new(1, 1);
        let (_machine, _block, process) = setup();

        scheduler.schedule(process.clone());

        assert!(scheduler.primary_pool.state.pop_global() == Some(process));
        assert!(scheduler.blocking_pool.state.pop_global().is_none());
    }

    #[test]
    fn test_schedule_on_blocking() {
        let scheduler = ProcessScheduler::new(1, 1);
        let (_machine, _block, process) = setup();

        process.set_blocking(true);
        scheduler.schedule(process.clone());

        assert!(scheduler.primary_pool.state.pop_global().is_none());
        assert!(scheduler.blocking_pool.state.pop_global() == Some(process));
    }

    #[test]
    fn test_schedule_pinned() {
        let scheduler = ProcessScheduler::new(2, 2);
        let (_machine, _block, process) = setup();

        process.set_thread_id(1);
        scheduler.schedule(process.clone());

        assert!(scheduler.primary_pool.state.pop_global().is_none());
        assert!(scheduler.blocking_pool.state.pop_global().is_none());
        assert!(scheduler.primary_pool.state.queues[1].has_external_jobs());
    }

    #[test]
    fn test_schedule_on_main_thread() {
        let scheduler = ProcessScheduler::new(2, 2);
        let (_machine, _block, process) = setup();

        scheduler.schedule_on_main_thread(process.clone());

        assert!(scheduler.primary_pool.state.pop_global().is_none());
        assert!(scheduler.blocking_pool.state.pop_global().is_none());
        assert!(scheduler.primary_pool.state.queues[0].has_external_jobs());
    }
}
