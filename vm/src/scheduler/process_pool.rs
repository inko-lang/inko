//! Thread pool for executing lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::process::RcProcess;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::process_worker::ProcessWorker;
use crate::scheduler::queue::RcQueue;
use crate::scheduler::worker::Worker;
use crate::vm::machine::Machine;
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

    /// Schedules a job onto a specific queue.
    pub fn schedule_onto_queue(&self, queue: usize, job: RcProcess) {
        self.state.schedule_onto_queue(queue, job);
    }

    /// Schedules a job onto the global queue.
    pub fn schedule(&self, job: RcProcess) {
        self.state.push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    pub fn terminate(&self) {
        self.state.terminate();
    }

    /// Starts the pool, blocking the current thread until the pool is
    /// terminated.
    ///
    /// The current thread will be used to perform jobs scheduled onto the first
    /// queue.
    pub fn start_main(&self, machine: Machine) -> JoinList<()> {
        let join_list = self.spawn_threads_for_range(1, machine.clone());
        let queue = self.state.queues[0].clone();

        ProcessWorker::new(0, queue, self.state.clone(), machine).run();

        join_list
    }

    /// Starts the pool, without blocking the calling thread.
    pub fn start(&self, machine: Machine) -> JoinList<()> {
        self.spawn_threads_for_range(0, machine)
    }

    /// Spawns OS threads for a range of queues, starting at the given position.
    fn spawn_threads_for_range(
        &self,
        start_at: usize,
        machine: Machine,
    ) -> JoinList<()> {
        let mut handles = Vec::new();

        for index in start_at..self.state.queues.len() {
            let handle = self.spawn_thread(
                index,
                machine.clone(),
                self.state.queues[index].clone(),
            );

            handles.push(handle);
        }

        JoinList::new(handles)
    }

    fn spawn_thread(
        &self,
        id: usize,
        machine: Machine,
        queue: RcQueue<RcProcess>,
    ) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("{} {}", self.name, id))
            .spawn(move || {
                ProcessWorker::new(id, queue, state, machine).run();
            })
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::test::setup;

    #[test]
    #[should_panic]
    fn test_new_with_zero_threads() {
        ProcessPool::new("test".to_string(), 0);
    }

    #[test]
    fn test_start_main() {
        let (machine, _block, process) = setup();
        let pool = &machine.state.scheduler.primary_pool;

        pool.schedule(process.clone());

        let threads = pool.start_main(machine.clone());

        threads.join().unwrap();

        assert_eq!(pool.state.is_alive(), false);
    }

    #[test]
    fn test_schedule_onto_queue() {
        let (machine, _block, process) = setup();
        let pool = &machine.state.scheduler.primary_pool;

        pool.schedule_onto_queue(0, process);

        assert!(pool.state.queues[0].has_external_jobs());
    }

    #[test]
    fn test_spawn_thread() {
        let (machine, _block, process) = setup();
        let pool = &machine.state.scheduler.primary_pool;

        let thread =
            pool.spawn_thread(0, machine.clone(), pool.state.queues[0].clone());

        pool.schedule(process.clone());

        thread.join().unwrap();

        assert_eq!(pool.state.has_global_jobs(), false);
    }
}
