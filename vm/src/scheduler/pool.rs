use crate::arc_without_weak::ArcWithoutWeak;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;
use crate::scheduler::worker::Worker;
use std::thread;

pub trait Pool<T: Send, W: Worker<T>> {
    fn state(&self) -> &ArcWithoutWeak<PoolState<T>>;

    /// Spawns a single OS thread that is to consume the given queue.
    fn spawn_thread<F>(
        &self,
        id: usize,
        queue: RcQueue<T>,
        callback: ArcWithoutWeak<F>,
    ) -> thread::JoinHandle<()>
    where
        F: Fn(&mut W, T) + Send + 'static;

    /// Schedules a job onto the global queue.
    fn schedule(&self, job: T) {
        self.state().push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    fn terminate(&self) {
        self.state().terminate();
    }

    /// Starts the pool, without blocking the calling thread.
    fn start<F>(&self, callback: F) -> JoinList<()>
    where
        F: Fn(&mut W, T) + Send + 'static,
    {
        let rc_callback = ArcWithoutWeak::new(callback);

        self.spawn_threads_for_range(0, &rc_callback)
    }

    /// Spawns OS threads for a range of queues, starting at the given position.
    fn spawn_threads_for_range<F>(
        &self,
        start_at: usize,
        callback: &ArcWithoutWeak<F>,
    ) -> JoinList<()>
    where
        F: Fn(&mut W, T) + Send + 'static,
    {
        let handles = self.state().queues[start_at..]
            .iter()
            .enumerate()
            .map(|(index, queue)| {
                // When using enumerate() with a range start > 0, the first
                // index is still 0.
                let worker_id = start_at + index;

                self.spawn_thread(worker_id, queue.clone(), callback.clone())
            })
            .collect();

        JoinList::new(handles)
    }
}
