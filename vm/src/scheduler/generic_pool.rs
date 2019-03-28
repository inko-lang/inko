//! Thread pool for executing generic tasks.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::scheduler::generic_worker::GenericWorker;
use crate::scheduler::pool::Pool;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;
use crate::scheduler::worker::Worker;
use std::thread;

/// A pool of threads for running generic tasks.
pub struct GenericPool<T: Send + 'static> {
    pub state: ArcWithoutWeak<PoolState<T>>,

    /// The base name of every thread in this pool.
    name: String,
}

impl<T: Send + 'static> GenericPool<T> {
    pub fn new(name: String, threads: usize) -> Self {
        assert!(
            threads > 0,
            "A GenericPool requires at least a single thread"
        );

        Self {
            name,
            state: ArcWithoutWeak::new(PoolState::new(threads)),
        }
    }
}

impl<T: Send + 'static> Pool<T, GenericWorker<T>> for GenericPool<T> {
    fn state(&self) -> &ArcWithoutWeak<PoolState<T>> {
        &self.state
    }

    fn spawn_thread<F>(
        &self,
        id: usize,
        queue: RcQueue<T>,
        callback: ArcWithoutWeak<F>,
    ) -> thread::JoinHandle<()>
    where
        F: Fn(&mut GenericWorker<T>, T) + Send + 'static,
    {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("{} {}", self.name, id))
            .spawn(move || {
                GenericWorker::new(queue, state).run(&*callback);
            })
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[test]
    #[should_panic]
    fn test_new_with_zero_threads() {
        GenericPool::<()>::new("test".to_string(), 0);
    }

    #[test]
    fn test_spawn_thread() {
        let pool = GenericPool::new("test".to_string(), 1);
        let number = ArcWithoutWeak::new(Mutex::new(0));
        let number_copy = number.clone();

        let callback = ArcWithoutWeak::new(
            move |worker: &mut GenericWorker<usize>, number| {
                *number_copy.lock() = number;
                worker.state().terminate();
            },
        );

        let thread =
            pool.spawn_thread(0, pool.state.queues[0].clone(), callback);

        pool.schedule(10);

        thread.join().unwrap();

        assert_eq!(*number.lock(), 10);
    }
}
