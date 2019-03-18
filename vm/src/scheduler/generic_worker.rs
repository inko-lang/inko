//! Workers for executing generic tasks.
use arc_without_weak::ArcWithoutWeak;
use scheduler::pool_state::PoolState;
use scheduler::queue::RcQueue;
use scheduler::worker::Worker;

/// A worker that can be used for executing a wide variety of tasks, instead of
/// being limited to only executing lightweight processes.
///
/// Generic workers do not support task pinning, or scheduling tasks directly
/// onto a specific worker.
pub struct GenericWorker<T: Send> {
    /// The queue owned by this worker.
    queue: RcQueue<T>,

    /// The state of the pool this worker belongs to.
    state: ArcWithoutWeak<PoolState<T>>,
}

impl<T: Send> GenericWorker<T> {
    pub fn new(queue: RcQueue<T>, state: ArcWithoutWeak<PoolState<T>>) -> Self {
        GenericWorker { queue, state }
    }
}

impl<T: Send> Worker<T> for GenericWorker<T> {
    fn run<F>(&mut self, callback: F)
    where
        F: Fn(&mut Self, T),
    {
        while self.state.is_alive() {
            if self.process_local_jobs(&callback) {
                continue;
            }

            if self.steal_from_other_queue() {
                continue;
            }

            if self.steal_from_global_queue() {
                continue;
            }

            self.state.park_while(|| !self.state.has_global_jobs());
        }
    }

    fn state(&self) -> &PoolState<T> {
        &self.state
    }

    fn queue(&self) -> &RcQueue<T> {
        &self.queue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashSet;

    fn numbers() -> (
        ArcWithoutWeak<Mutex<HashSet<usize>>>,
        ArcWithoutWeak<Mutex<HashSet<usize>>>,
    ) {
        let orig = ArcWithoutWeak::new(Mutex::new(HashSet::new()));
        let copy = orig.clone();

        (orig, copy)
    }

    fn worker() -> GenericWorker<usize> {
        let state = ArcWithoutWeak::new(PoolState::new(2));

        GenericWorker::new(state.queues[0].clone(), state)
    }

    #[test]
    fn test_run_global_jobs() {
        let (numbers, numbers_copy) = numbers();
        let mut worker = worker();

        worker.state.push_global(10);

        worker.run(move |worker, number| {
            numbers_copy.lock().insert(number);
            worker.state.terminate();
        });

        assert!(numbers.lock().contains(&10));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_steal_then_terminate() {
        let (numbers, numbers_copy) = numbers();
        let mut worker = worker();

        worker.state.queues[1].push_internal(10);

        worker.run(move |worker, number| {
            numbers_copy.lock().insert(number);
            worker.state.terminate();
        });

        assert!(numbers.lock().contains(&10));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_steal_then_work() {
        let (numbers, numbers_copy) = numbers();
        let mut worker = worker();

        worker.state.queues[1].push_internal(10);

        // Here the order of work is:
        //
        // 1. Steal from other queue
        // 2. Go back to processing our own queue
        // 3. Terminate
        worker.run(move |worker, number| {
            numbers_copy.lock().insert(number);

            worker.queue.push_internal(20);

            if number == 20 {
                worker.state.terminate();
            }
        });

        assert!(numbers.lock().contains(&10));
        assert!(numbers.lock().contains(&20));
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_work_then_terminate_steal_loop() {
        let (numbers, numbers_copy) = numbers();
        let mut worker = worker();

        worker.state.queues[0].push_internal(10);
        worker.state.queues[1].push_internal(20);

        worker.run(move |worker, number| {
            numbers_copy.lock().insert(number);
            worker.state.terminate();
        });

        assert_eq!(numbers.lock().contains(&10), true);
        assert_eq!(numbers.lock().contains(&20), false);

        assert!(worker.state.queues[1].has_local_jobs());
    }
}
