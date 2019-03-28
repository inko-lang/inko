use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;

/// A trait providing the basic building blocks of a worker thread.
pub trait Worker<T: Send> {
    /// Starts the worker, blocking the current thread until the worker
    /// terminates.
    fn run<F>(&mut self, callback: F)
    where
        F: Fn(&mut Self, T);

    fn state(&self) -> &PoolState<T>;

    fn queue(&self) -> &RcQueue<T>;

    /// Processes all local jobs until we run out of work.
    ///
    /// This method returns true if the worker should self terminate.
    fn process_local_jobs<F>(&mut self, callback: &F) -> bool
    where
        F: Fn(&mut Self, T),
    {
        loop {
            if !self.state().is_alive() {
                return true;
            }

            if let Some(job) = self.queue().pop() {
                callback(self, job);
            } else {
                return false;
            }
        }
    }

    fn steal_from_other_queue(&self) -> bool {
        // We may try to steal from our queue, but that's OK because it's empty
        // and none of the below operations are blocking.
        for queue in &self.state().queues {
            if queue.steal_into(&self.queue()) {
                return true;
            }
        }

        false
    }

    /// Steals a single job from the global queue.
    ///
    /// This method will return `true` if a job was stolen.
    fn steal_from_global_queue(&self) -> bool {
        if let Some(job) = self.state().pop_global() {
            self.queue().push_internal(job);
            true
        } else {
            false
        }
    }
}
