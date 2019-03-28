//! Executing of lightweight Inko processes in a single thread.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::process::RcProcess;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;
use crate::scheduler::worker::Worker;

/// The state that a worker is in.
#[derive(Eq, PartialEq, Debug)]
pub enum Mode {
    /// The worker should process its own queue or other queues in a normal
    /// fashion.
    Normal,

    /// The worker should only process a particular job, and not steal any other
    /// jobs.
    Exclusive,
}

/// A worker owned by a thread, used for executing jobs from a scheduler queue.
pub struct ProcessWorker {
    /// The unique ID of this worker, used for pinning jobs.
    pub id: usize,

    /// The queue owned by this worker.
    queue: RcQueue<RcProcess>,

    /// The state of the pool this worker belongs to.
    state: ArcWithoutWeak<PoolState<RcProcess>>,

    /// The mode this worker is in.
    mode: Mode,
}

impl ProcessWorker {
    /// Starts a new worker operating in the normal mode.
    pub fn new(
        id: usize,
        queue: RcQueue<RcProcess>,
        state: ArcWithoutWeak<PoolState<RcProcess>>,
    ) -> Self {
        ProcessWorker {
            id,
            queue,
            state,
            mode: Mode::Normal,
        }
    }

    /// Changes the worker state so it operates in exclusive mode.
    ///
    /// When in exclusive mode, only the currently running job will be allowed
    /// to run on this worker. All other jobs are pushed back into the global
    /// queue.
    pub fn enter_exclusive_mode(&mut self) {
        self.queue.move_external_jobs();

        while let Some(job) = self.queue.pop() {
            self.state.push_global(job);
        }

        self.mode = Mode::Exclusive;
    }

    pub fn leave_exclusive_mode(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Performs a single iteration of the normal work loop.
    fn normal_iteration<F>(&mut self, callback: &F)
    where
        F: Fn(&mut Self, RcProcess),
    {
        if self.process_local_jobs(callback) {
            return;
        }

        if self.steal_from_other_queue() {
            return;
        }

        if self.queue.move_external_jobs() {
            return;
        }

        if self.steal_from_global_queue() {
            return;
        }

        self.state.park_while(|| {
            !self.state.has_global_jobs() && !self.queue.has_external_jobs()
        });
    }

    /// Runs a single iteration of an exclusive work loop.
    fn exclusive_iteration<F>(&mut self, callback: &F)
    where
        F: Fn(&mut Self, RcProcess),
    {
        if self.process_local_jobs(callback) {
            return;
        }

        // Moving external jobs would allow other workers to steal them,
        // starving the current worker of pinned jobs. Since only one job can be
        // pinned to a worker, we don't need a loop here.
        if let Some(job) = self.queue.pop_external_job() {
            callback(self, job);
            return;
        }

        self.state.park_while(|| !self.queue.has_external_jobs());
    }
}

impl Worker<RcProcess> for ProcessWorker {
    fn state(&self) -> &PoolState<RcProcess> {
        &self.state
    }

    fn queue(&self) -> &RcQueue<RcProcess> {
        &self.queue
    }

    /// Starts the worker, blocking the calling thread.
    ///
    /// This method will not return until our queue or any other queues this
    /// worker has access to are terminated.
    fn run<F>(&mut self, callback: F)
    where
        F: Fn(&mut Self, RcProcess),
    {
        while self.state.is_alive() {
            match self.mode {
                Mode::Normal => self.normal_iteration(&callback),
                Mode::Exclusive => self.exclusive_iteration(&callback),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::process;
    use crate::vm::test::setup;
    use parking_lot::Mutex;
    use std::collections::HashSet;

    fn pids_set() -> (
        ArcWithoutWeak<Mutex<HashSet<usize>>>,
        ArcWithoutWeak<Mutex<HashSet<usize>>>,
    ) {
        let orig = ArcWithoutWeak::new(Mutex::new(HashSet::new()));
        let copy = orig.clone();

        (orig, copy)
    }

    fn worker() -> ProcessWorker {
        let state = ArcWithoutWeak::new(PoolState::new(2));

        ProcessWorker::new(0, state.queues[0].clone(), state)
    }

    #[test]
    fn test_run_global_jobs() {
        let (pids, pids_copy) = pids_set();
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.push_global(process.clone());

        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());
            worker.state.terminate();
        });

        assert!(pids.lock().contains(&process.identifier()));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_with_external_jobs() {
        let (pids, pids_copy) = pids_set();
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.queues[0].push_external(process.clone());

        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());
            worker.state.terminate();
        });

        assert!(pids.lock().contains(&process.identifier()));
    }

    #[test]
    fn test_run_steal_then_terminate() {
        let (pids, pids_copy) = pids_set();
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.queues[1].push_internal(process.clone());

        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());
            worker.state.terminate();
        });

        assert!(pids.lock().contains(&process.identifier()));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_steal_then_work() {
        let (pids, pids_copy) = pids_set();
        let (machine, block, process) = setup();
        let process2 = process::allocate(&machine.state, &block);
        let process2_clone = process2.clone();
        let mut worker = worker();

        process.set_main();
        worker.state.queues[1].push_internal(process.clone());

        // Here the order of work is:
        //
        // 1. Steal from other queue
        // 2. Go back to processing our own queue
        // 3. Terminate
        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());

            worker.queue.push_internal(process2_clone.clone());

            if !process.is_main() {
                worker.state.terminate();
            }
        });

        assert!(pids.lock().contains(&process.identifier()));
        assert!(pids.lock().contains(&process2.identifier()));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_work_then_terminate_steal_loop() {
        let (pids, pids_copy) = pids_set();
        let (machine, block, process) = setup();
        let process2 = process::allocate(&machine.state, &block);
        let mut worker = worker();

        worker.state.queues[0].push_internal(process.clone());
        worker.state.queues[1].push_internal(process2.clone());

        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());
            worker.state.terminate();
        });

        assert_eq!(pids.lock().contains(&process.identifier()), true);
        assert_eq!(pids.lock().contains(&process2.identifier()), false);

        assert!(worker.state.queues[1].has_local_jobs());
    }

    #[test]
    fn test_run_exclusive_iteration() {
        let (pids, pids_copy) = pids_set();
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.enter_exclusive_mode();
        worker.queue.push_external(process.clone());

        worker.run(move |worker, process| {
            pids_copy.lock().insert(process.identifier());
            worker.state.terminate();
        });

        assert!(pids.lock().contains(&process.identifier()));
    }

    #[test]
    fn test_enter_exclusive_mode() {
        let mut worker = worker();
        let (machine, block, process) = setup();
        let process2 = process::allocate(&machine.state, &block);

        worker.queue.push_internal(process);
        worker.queue.push_external(process2);
        worker.enter_exclusive_mode();

        assert_eq!(worker.mode, Mode::Exclusive);
        assert_eq!(worker.queue.has_local_jobs(), false);
        assert!(worker.queue.pop_external_job().is_none());
    }

    #[test]
    fn test_leave_exclusive_mode() {
        let mut worker = worker();

        worker.enter_exclusive_mode();
        worker.leave_exclusive_mode();

        assert_eq!(worker.mode, Mode::Normal);
    }
}
