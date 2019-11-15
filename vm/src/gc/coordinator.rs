//! Types for starting and coordinating garbage collecting of a process.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::gc::collection::Collection;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;
use crate::scheduler::worker::Worker as WorkerTrait;
use crate::vm::state::RcState;
use std::thread;

/// A worker used for coordinating the garbage collecting a process.
pub struct Worker {
    /// The queue owned by this worker.
    queue: RcQueue<Collection>,

    /// The state of the pool this worker belongs to.
    state: ArcWithoutWeak<PoolState<Collection>>,

    /// The VM state this worker belongs to.
    vm_state: RcState,
}

impl Worker {
    pub fn new(
        queue: RcQueue<Collection>,
        state: ArcWithoutWeak<PoolState<Collection>>,
        vm_state: RcState,
    ) -> Self {
        Worker {
            queue,
            state,
            vm_state,
        }
    }
}

impl WorkerTrait<Collection> for Worker {
    fn state(&self) -> &PoolState<Collection> {
        &self.state
    }

    fn queue(&self) -> &RcQueue<Collection> {
        &self.queue
    }

    fn process_job(&mut self, job: Collection) {
        job.perform(&self.vm_state);
    }
}

/// A pool of threads for coordinating the garbage collecting of processes.
pub struct Pool {
    state: ArcWithoutWeak<PoolState<Collection>>,
}

impl Pool {
    pub fn new(threads: usize) -> Self {
        assert!(threads > 0, "GC pools require at least a single thread");

        Self {
            state: ArcWithoutWeak::new(PoolState::new(threads)),
        }
    }

    /// Schedules a job onto the global queue.
    pub fn schedule(&self, job: Collection) {
        self.state.push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    pub fn terminate(&self) {
        self.state.terminate();
    }

    /// Starts the pool, without blocking the calling thread.
    pub fn start(&self, vm_state: RcState) -> JoinList<()> {
        let handles = self
            .state
            .queues
            .iter()
            .enumerate()
            .map(|(index, queue)| {
                self.spawn_thread(index, queue.clone(), vm_state.clone())
            })
            .collect();

        JoinList::new(handles)
    }

    fn spawn_thread(
        &self,
        id: usize,
        queue: RcQueue<Collection>,
        vm_state: RcState,
    ) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("GC {}", id))
            .spawn(move || {
                Worker::new(queue, state, vm_state).run();
            })
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::gc::collection::Collection;
    use crate::vm::state::State;
    use crate::vm::test::setup;

    fn worker() -> Worker {
        let state = ArcWithoutWeak::new(PoolState::new(2));
        let vm_state = State::with_rc(Config::new(), &[]);

        Worker::new(state.queues[0].clone(), state, vm_state)
    }

    #[test]
    fn test_worker_run_global_jobs() {
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.push_global(Collection::new(process));
        worker.run();

        assert_eq!(worker.state.has_global_jobs(), false);
    }

    #[test]
    fn test_worker_run_steal_then_terminate() {
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.queues[1].push_internal(Collection::new(process));
        worker.run();

        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_worker_run_steal_then_work() {
        let (_machine, _block, process) = setup();
        let mut worker = worker();

        worker.state.queues[1].push_internal(Collection::new(process.clone()));
        worker.queue.push_internal(Collection::new(process));
        worker.run();

        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
        assert_eq!(worker.queue.has_local_jobs(), false);
    }

    #[test]
    #[should_panic]
    fn test_pool_new_with_zero_threads() {
        Pool::new(0);
    }

    #[test]
    fn test_pool_spawn_thread() {
        let (machine, _block, process) = setup();
        let pool = Pool::new(1);

        pool.schedule(Collection::new(process));

        let thread = pool.spawn_thread(
            0,
            pool.state.queues[0].clone(),
            machine.state.clone(),
        );

        thread.join().unwrap();

        assert_eq!(pool.state.has_global_jobs(), false);
    }
}
