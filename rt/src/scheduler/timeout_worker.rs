//! Rescheduling of processes with expired timeouts.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::process::ProcessPointer;
use crate::scheduler::process::Scheduler;
use crate::scheduler::timeouts::{Timeout, Timeouts};
use crate::state::State;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// The shortest amount of time we'll sleep for when timeouts are present, in
/// milliseconds.
const MIN_SLEEP_TIME: u64 = 10;

/// The initial capacity of the shared and local queues.
const QUEUE_START_CAPACITY: usize = 1024 / size_of::<Message>();

/// The percentage of timeouts (from 0.0 to 1.0) that can be invalidated before
/// the timeouts heap must be cleaned up.
const FRAGMENTATION_THRESHOLD: f64 = 0.1;

struct Message {
    process: ProcessPointer,
    timeout: ArcWithoutWeak<Timeout>,
}

/// The inner part of a worker, only accessible by the owning thread.
struct Inner {
    /// The processes suspended with timeouts.
    timeouts: Timeouts,

    /// The messages to process.
    queue: VecDeque<Message>,
}

/// A TimeoutWorker is tasked with rescheduling processes when their timeouts
/// expire.
///
/// ## Cleaning up of invalid timeouts
///
/// Processes will reschedule other processes when they send a message, and the
/// receiving process is suspended. When a timeout is used, the sending process
/// will invalidate it. This can result in the internal list of timeouts
/// building up lots of invalidated timeouts over time, depending on the
/// expiration time of timeouts that preceed these invalidated timeouts.
///
/// To resolve this, the internal list of timeouts is cleaned up periodically.
/// This is done by the timeout worker itself, not by processes sending
/// messages. This ensures this cleanup work does not impact threads running
/// processes.
pub(crate) struct TimeoutWorker {
    /// The inner part of the rescheduler that can only be used by the thread
    /// that reschedules processes.
    inner: UnsafeCell<Inner>,

    /// The queue of messages to process.
    ///
    /// Messages in this queue are periodically moved into the local queue,
    /// allowing us to process the messages with minimal locking.
    queue: Mutex<VecDeque<Message>>,

    /// A condition variable to use for waking up the worker when it's sleeping.
    cvar: Condvar,

    /// The number of timeouts that have been invalidated by sending a message
    /// to the process, before the timeout expired.
    expired: AtomicUsize,
}

unsafe impl Sync for TimeoutWorker {}

impl TimeoutWorker {
    pub(crate) fn new() -> Self {
        TimeoutWorker {
            inner: UnsafeCell::new(Inner {
                timeouts: Timeouts::new(),
                queue: VecDeque::with_capacity(QUEUE_START_CAPACITY),
            }),
            queue: Mutex::new(VecDeque::with_capacity(QUEUE_START_CAPACITY)),
            cvar: Condvar::new(),
            expired: AtomicUsize::new(0),
        }
    }

    pub(crate) fn increase_expired_timeouts(&self) {
        self.expired.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn run(&self, state: &State) {
        while state.scheduler.is_alive() {
            let timeout = self.run_iteration(state);

            self.sleep(&state.scheduler, timeout);
        }
    }

    pub(crate) fn suspend(
        &self,
        process: ProcessPointer,
        timeout: ArcWithoutWeak<Timeout>,
    ) {
        let mut queue = self.queue.lock().unwrap();

        queue.push_back(Message { process, timeout });
        self.cvar.notify_one();
    }

    fn run_iteration(&self, state: &State) -> Option<Duration> {
        self.move_messages();
        self.defragment_heap();
        self.handle_pending_messages();

        if let Some(time) = self.reschedule_expired_processes(state) {
            if time.as_millis() < (MIN_SLEEP_TIME as u128) {
                Some(Duration::from_millis(MIN_SLEEP_TIME))
            } else {
                Some(time)
            }
        } else {
            None
        }
    }

    fn sleep(&self, scheduler: &Scheduler, timeout: Option<Duration>) {
        let mut queue = self.queue.lock().unwrap();

        while queue.is_empty() && scheduler.is_alive() {
            if let Some(time) = timeout {
                let result = self.cvar.wait_timeout(queue, time).unwrap();

                if result.1.timed_out() {
                    break;
                } else {
                    queue = result.0;
                }
            } else {
                queue = self.cvar.wait(queue).unwrap();
            }
        }
    }

    fn reschedule_expired_processes(&self, state: &State) -> Option<Duration> {
        let inner = self.inner_mut();
        let (expired, time_until_expiration) =
            inner.timeouts.processes_to_reschedule(state);

        state.scheduler.schedule_multiple(expired);
        time_until_expiration
    }

    fn handle_pending_messages(&self) {
        while let Some(msg) = self.inner_mut().queue.pop_front() {
            self.inner_mut().timeouts.insert(msg.process, msg.timeout);
        }
    }

    fn move_messages(&self) {
        self.inner_mut().queue.append(&mut self.queue.lock().unwrap());
    }

    fn defragment_heap(&self) {
        let fragmented = self.expired.load(Ordering::Acquire) as f64
            / self.inner().timeouts.len() as f64;

        if fragmented < FRAGMENTATION_THRESHOLD {
            return;
        }

        let removed = self.inner_mut().timeouts.remove_invalid_entries();

        self.expired.fetch_sub(removed, Ordering::AcqRel);
    }

    fn inner(&self) -> &Inner {
        unsafe { &*self.inner.get() }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    fn inner_mut(&self) -> &mut Inner {
        unsafe { &mut *self.inner.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Process;
    use crate::stack::Stack;
    use crate::test::{empty_process_class, new_process, setup};

    #[test]
    fn test_new() {
        let worker = TimeoutWorker::new();

        assert_eq!(worker.inner().timeouts.len(), 0);
        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
    }

    #[test]
    fn test_suspend() {
        let state = setup();
        let worker = TimeoutWorker::new();
        let class = empty_process_class("A");
        let process = new_process(*class);

        worker.suspend(
            *process,
            Timeout::duration(&state, Duration::from_secs(1)),
        );

        assert!(!worker.queue.lock().unwrap().is_empty());
    }

    #[test]
    fn test_increase_expired_timeouts() {
        let worker = TimeoutWorker::new();

        worker.increase_expired_timeouts();

        assert_eq!(worker.expired.load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_run_with_fragmented_heap() {
        let state = setup();
        let class = empty_process_class("A");
        let process = Process::alloc(*class, Stack::new(1024));
        let worker = TimeoutWorker::new();

        for time in &[10_u64, 5_u64] {
            let timeout = Timeout::duration(&state, Duration::from_secs(*time));

            process.state().waiting_for_channel(Some(timeout.clone()));
            worker.suspend(process, timeout);
        }

        worker.increase_expired_timeouts();

        // This makes sure the timeouts are present before we start the run
        // loop.
        worker.move_messages();
        worker.handle_pending_messages();
        worker.run_iteration(&state);

        assert_eq!(worker.inner().timeouts.len(), 1);
        assert_eq!(worker.expired.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_run_with_message() {
        let state = setup();
        let class = empty_process_class("A");
        let process = Process::alloc(*class, Stack::new(1024));
        let worker = TimeoutWorker::new();
        let timeout = Timeout::duration(&state, Duration::from_secs(10));

        process.state().waiting_for_channel(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.run_iteration(&state);

        assert_eq!(worker.inner().timeouts.len(), 1);
    }

    #[test]
    fn test_run_with_reschedule() {
        let state = setup();
        let class = empty_process_class("A");
        let process = Process::alloc(*class, Stack::new(1024));
        let worker = TimeoutWorker::new();
        let timeout = Timeout::duration(&state, Duration::from_secs(0));

        process.state().waiting_for_channel(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.run_iteration(&state);

        assert_eq!(worker.inner().timeouts.len(), 0);
    }

    #[test]
    fn test_defragment_heap_without_fragmentation() {
        let state = setup();
        let class = empty_process_class("A");
        let process = Process::alloc(*class, Stack::new(1024));
        let worker = TimeoutWorker::new();
        let timeout = Timeout::duration(&state, Duration::from_secs(1));

        process.state().waiting_for_channel(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.move_messages();
        worker.handle_pending_messages();
        worker.defragment_heap();

        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
        assert_eq!(worker.inner().timeouts.len(), 1);
    }

    #[test]
    fn test_defragment_heap_with_fragmentation() {
        let state = setup();
        let class = empty_process_class("A");
        let process = Process::alloc(*class, Stack::new(1024));
        let worker = TimeoutWorker::new();

        for time in &[1_u64, 1_u64] {
            let timeout = Timeout::duration(&state, Duration::from_secs(*time));

            process.state().waiting_for_channel(Some(timeout.clone()));
            worker.suspend(process, timeout);
        }

        worker.increase_expired_timeouts();
        worker.move_messages();
        worker.handle_pending_messages();
        worker.defragment_heap();

        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
        assert_eq!(worker.inner().timeouts.len(), 1);
    }
}
