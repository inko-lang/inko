//! Scheduling and execution of lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::machine::Machine;
use crate::mem::{ClassPointer, MethodPointer};
use crate::process::{Process, ProcessPointer};
use crate::state::State;
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::atomic::AtomicCell;
use crossbeam_utils::thread::scope;
use rand::rngs::ThreadRng;
use rand::thread_rng;
use std::cmp::min;
use std::collections::VecDeque;
use std::mem::size_of;
use std::ops::Drop;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// The starting capacity of the global queue.
///
/// The global queue grows if needed, this capacity simply exists to reduce the
/// amount of allocations without wasting
const GLOBAL_QUEUE_START_CAPACITY: usize = 8192 / size_of::<ProcessPointer>();

/// The maximum number of jobs we can store in a local queue before overflowing
/// to the global queue.
///
/// The exact value here doesn't really matter as other threads may steal from
/// our local queue, as long as the value is great enough to avoid excessive
/// overflowing in most common cases.
const LOCAL_QUEUE_CAPACITY: usize = 2048 / size_of::<ProcessPointer>();

/// The maximum number of jobs to steal at a time.
///
/// This puts an upper bound on the time spent stealing from a single queue.
const STEAL_LIMIT: usize = 32;

/// The blocking epoch to start at.
const START_EPOCH: u64 = 1;

/// The epoch value that indicates a thread isn't blocking.
const NOT_BLOCKING: u64 = 0;

/// The interval (in microseconds) at which the monitor thread runs.
///
/// Threads found to have been blocking for longer than this interval will be
/// replaced with a backup thread.
///
/// This value is mostly arbitrary. A greater value would reduce CPU usage at
/// the cost of blocking operations blocking an OS thread for longer. A lower
/// value has the opposite effect. This value seemed like a reasonable
/// compromise.
///
/// Note that the actual interval may be greater, as some OS' have a timer
/// resolution of e.g. 1 millisecond. Most notably, Windows seems to enforce a
/// minimum of around 15 milliseconds:
///
/// - https://github.com/rust-lang/rust/issues/43376
/// - https://github.com/tokio-rs/tokio/issues/5021
///
/// In addition, it may take a little longer than this time before a thread is
/// marked as being too slow.
///
/// All of this is fine as our goal isn't to guarantee blocking operations never
/// take more than the given interval. Instead, our goal is to ensure blocking
/// operations don't block an OS thread indefinitely.
const MONITOR_INTERVAL: u64 = 100;

/// The maximum amount of regular sleep cycles to perform (without finding
/// blocking processes) before entering a deep sleep.
///
/// Waking up a monitor from a deep sleep requires the use of a mutex, which
/// incurs a cost on threads entering a blocking operation. To reduce this cost
/// we perform a number of regular cycles before entering a deep sleep.
const MAX_IDLE_CYCLES: u64 = 1_000_000 / MONITOR_INTERVAL;

/// The shared half of a thread.
struct Shared {
    /// The queue threads can steal work from.
    queue: ArcWithoutWeak<ArrayQueue<ProcessPointer>>,

    /// The epoch at which this thread started a blocking operation.
    ///
    /// A value of zero indicates the thread isn't blocking.
    blocked_at: AtomicU64,
}

/// The private half of a thread, used only by the OS thread this state belongs
/// to.
pub(crate) struct Thread<'a> {
    /// The unique ID of this thread.
    ///
    /// This is used to prevent a thread from trying to steal work from itself,
    /// which is redundant.
    id: usize,

    /// The thread-local queue new work is scheduled onto, unless we consider it
    /// to be too full.
    work: ArcWithoutWeak<ArrayQueue<ProcessPointer>>,

    /// The pool this thread belongs to.
    pool: &'a Pool,

    /// A process to run before trying to consume any other work.
    ///
    /// Sometimes we reschedule a process and want to run it immediately,
    /// instead of processing jobs in the local queue. For example, when sending
    /// a message to a process not doing anything, running it immediately
    /// reduces the latency of producing a response to the message.
    ///
    /// Other threads can't steal from this slot, because that would defeat its
    /// purpose.
    priority: Option<ProcessPointer>,

    /// A flag indicating this thread is a backup thread.
    backup: bool,

    /// The epoch at which we started blocking.
    ///
    /// This value mirrors `Shared.blocked_at` and is used to detect if a
    /// monitor thread changed the status.
    ///
    /// A value of 0 indicates the thread isn't blocked.
    blocked_at: u64,

    /// A random number generator to use for the current thread.
    pub(crate) rng: ThreadRng,
}

impl<'a> Thread<'a> {
    fn new(id: usize, pool: &'a Pool) -> Thread {
        Self {
            id,
            work: pool.threads[id].queue.clone(),
            priority: None,
            pool,
            backup: false,
            blocked_at: NOT_BLOCKING,
            rng: thread_rng(),
        }
    }

    fn backup(pool: &'a Pool) -> Thread {
        Self {
            // For backup threads the ID/queue doesn't matter, because we won't
            // use them until we're turned into a regular thread.
            id: 0,
            work: pool.threads[0].queue.clone(),
            priority: None,
            pool,
            backup: true,
            blocked_at: NOT_BLOCKING,
            rng: thread_rng(),
        }
    }

    pub(crate) fn schedule(&mut self, process: ProcessPointer) {
        if let Err(process) = self.work.push(process) {
            self.pool.schedule(process);
            return;
        }

        if self.work.len() > 1 && self.pool.sleeping() > 0 {
            self.pool.notify_one();
        }
    }

    pub(crate) fn schedule_priority(&mut self, process: ProcessPointer) {
        if self.backup {
            self.schedule(process);
        } else {
            // Outside of any bugs in the VM, we should never reach this point
            // and still have a value in the priority slot, so we can just set
            // the value as-is.
            self.priority = Some(process);
        }
    }

    pub(crate) fn start_blocking(&mut self) {
        // Pushing all our work to other threads may take some time. To reduce
        // the latency of the blocking operation we instead have other threads
        // steal work from us. Since threads can't steal from the priority slot,
        // we push this process back into the local queue.
        if let Some(process) = self.priority.take() {
            if let Err(process) = self.work.push(process) {
                self.pool.schedule(process);
            }
        }

        if let Some(process) = self.work.pop() {
            // Moving a single job is enough to wake up at least a single
            // sleeping thread (if any), without slowing down the blocking
            // operation further. If we _just_ signalled sleeping threads we
            // might end up notifying them _after_ they perform their checks,
            // resulting in them never waking up.
            self.pool.schedule(process);
        }

        let epoch = self.pool.current_epoch();
        let shared = &self.pool.threads[self.id];

        self.blocked_at = epoch;
        shared.blocked_at.store(epoch, Ordering::Release);

        // The monitor thread may be sleeping indefinitely if we're the first
        // blocking thread in a while, so we have to make sure it wakes up.
        let status = self.pool.monitor.status.load();

        if status == MonitorStatus::Sleeping
            && self
                .pool
                .monitor
                .status
                .compare_exchange(status, MonitorStatus::Notified)
                .is_ok()
        {
            let _lock = self.pool.monitor.lock.lock().unwrap();

            self.pool.monitor.cvar.notify_one();
        }
    }

    pub(crate) fn finish_blocking(&mut self) {
        let shared = &self.pool.threads[self.id];
        let epoch = self.blocked_at;

        if shared
            .blocked_at
            .compare_exchange(
                epoch,
                NOT_BLOCKING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            // The monitor thread determined we took too long and we have to
            // become a backup thread.
            self.backup = true;
            self.blocked_at = NOT_BLOCKING;
        }
    }

    pub(crate) fn blocking<F, R>(&mut self, function: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.start_blocking();

        let res = function();

        self.finish_blocking();
        res
    }

    fn move_work_to_global_queue(&mut self) {
        let len = self.work.len() + if self.priority.is_some() { 1 } else { 0 };

        if len == 0 {
            return;
        }

        let mut work = Vec::with_capacity(len);

        if let Some(process) = self.priority.take() {
            work.push(process);
        }

        while let Some(process) = self.work.pop() {
            work.push(process);
        }

        self.pool.schedule_multiple(work);
    }

    fn run(&mut self, state: &State) {
        while self.pool.is_alive() {
            if self.backup {
                // When finishing a blocking operation the process is allowed to
                // continue running, as rescheduling it would likely slow it
                // down even further. This means new work may have been produced
                // since we entered a blocking operation. If we don't push this
                // work back to the global queue, it may never be completed
                // (e.g. if all other threads are asleep).
                self.move_work_to_global_queue();

                let mut blocked = self.pool.blocked_threads.lock().unwrap();

                if let Some(id) = blocked.pop_front() {
                    self.backup = false;
                    self.id = id;
                    self.work = self.pool.threads[id].queue.clone();
                } else {
                    // The pool state may have been changed. If we don't check
                    // it here we may never terminate.
                    if !self.pool.is_alive() {
                        return;
                    }

                    let _result = self.pool.blocked_cvar.wait(blocked).unwrap();

                    continue;
                }
            }

            if let Some(process) = self.next_local_process() {
                self.run_process(state, process);
                continue;
            }

            if let Some(process) = self.steal_from_thread() {
                self.run_process(state, process);
                continue;
            }

            if let Some(process) = self.steal_from_global() {
                self.run_process(state, process);
                continue;
            }

            self.sleep();
        }
    }

    fn next_local_process(&mut self) -> Option<ProcessPointer> {
        self.priority.take().or_else(|| self.work.pop())
    }

    fn steal_from_thread(&mut self) -> Option<ProcessPointer> {
        // We start stealing at the thread that comes after ours, wrapping
        // around as needed.
        //
        // While implementing the scheduler we looked into stealing from a
        // random start index instead, but didn't observe any performance
        // benefits.
        let start = self.id + 1;
        let len = self.pool.threads.len();

        for index in 0..len {
            let index = (start + index) % len;

            // We don't want to steal from ourselves, because there's nothing to
            // steal.
            if index == self.id {
                continue;
            }

            let steal_from = &self.pool.threads[index];

            if let Some(initial) = steal_from.queue.pop() {
                let len = steal_from.queue.len();
                let steal = min(len / 2, STEAL_LIMIT);

                for _ in 0..steal {
                    if let Some(process) = steal_from.queue.pop() {
                        if let Err(process) = self.work.push(process) {
                            self.pool.schedule(process);
                            break;
                        }
                    } else {
                        break;
                    }
                }

                return Some(initial);
            }
        }

        None
    }

    fn steal_from_global(&mut self) -> Option<ProcessPointer> {
        let mut global = self.pool.global.lock().unwrap();

        if let Some(initial) = global.pop() {
            let len = global.len();
            let steal = min(len / 2, STEAL_LIMIT);

            if steal > 0 {
                // We're splitting at an index, so we must subtract one from the
                // amount.
                for process in global.split_off(steal - 1) {
                    if let Err(process) = self.work.push(process) {
                        global.push(process);
                        break;
                    }
                }
            }

            Some(initial)
        } else {
            None
        }
    }

    fn sleep(&self) {
        let global = self.pool.global.lock().unwrap();

        if !global.is_empty() || !self.pool.is_alive() {
            return;
        }

        self.pool.sleeping.fetch_add(1, Ordering::AcqRel);

        // We don't handle spurious wakeups here because:
        //
        // 1. We may be woken up when new work is produced on a local queue,
        //    while the global queue is still empty
        // 2. If we're woken up too early we'll just perform another work
        //    iteration, then go back to sleep.
        let _result = self.pool.sleeping_cvar.wait(global).unwrap();

        self.pool.sleeping.fetch_sub(1, Ordering::AcqRel);
    }

    fn run_process(&mut self, state: &State, process: ProcessPointer) {
        Machine::new(state).run(self, process);
    }
}

impl<'a> Drop for Thread<'a> {
    fn drop(&mut self) {
        while let Some(process) = self.work.pop() {
            Process::drop_and_deallocate(process);
        }

        if let Some(process) = self.priority.take() {
            Process::drop_and_deallocate(process);
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u8)]
enum MonitorStatus {
    Normal,
    Notified,
    Sleeping,
}

/// A thread that monitors the thread pool, replacing any blocking threads with
/// backup threads.
struct Monitor<'a> {
    /// The minimum time between checks.
    interval: Duration,

    /// The current epoch.
    ///
    // This value mimics the epoch tracked in the Pool, and is used so we can
    // check/update the epoch value without having to use atomic operations for
    // everything.
    epoch: u64,

    /// The pool we're monitoring.
    pool: &'a Pool,
}

impl<'a> Monitor<'a> {
    fn new(pool: &'a Pool) -> Self {
        Self {
            epoch: START_EPOCH,
            interval: Duration::from_micros(MONITOR_INTERVAL),
            pool,
        }
    }

    fn run(&mut self) {
        let mut idle_cycles = 0;

        while self.pool.is_alive() {
            let found_blocking = self.check_threads();

            self.update_epoch();

            if found_blocking {
                idle_cycles = 0;
                self.sleep();
            } else if idle_cycles < MAX_IDLE_CYCLES {
                idle_cycles += 1;
                self.sleep();
            } else {
                idle_cycles = 0;
                self.deep_sleep();
            }
        }
    }

    fn check_threads(&self) -> bool {
        let mut found_blocking = false;

        for (id, thread) in self.pool.threads.iter().enumerate() {
            let thread_epoch = thread.blocked_at.load(Ordering::Acquire);

            if thread_epoch == NOT_BLOCKING {
                continue;
            }

            found_blocking = true;

            if thread_epoch == self.epoch {
                continue;
            }

            let result = thread.blocked_at.compare_exchange(
                thread_epoch,
                NOT_BLOCKING,
                Ordering::AcqRel,
                Ordering::Acquire,
            );

            if result.is_ok() {
                let mut blocked = self.pool.blocked_threads.lock().unwrap();

                blocked.push_back(id);
                self.pool.blocked_cvar.notify_one();
            }
        }

        found_blocking
    }

    fn update_epoch(&mut self) {
        if self.epoch == u64::MAX {
            self.epoch = START_EPOCH;
        } else {
            self.epoch += 1;
        }

        self.pool.epoch.store(self.epoch, Ordering::Release);
    }

    fn sleep(&self) {
        let sleep_at = Instant::now();
        let mut timeout = self.interval;

        self.pool.monitor.status.store(MonitorStatus::Normal);

        let mut lock = self.pool.monitor.lock.lock().unwrap();

        while self.pool.is_alive() {
            let result =
                self.pool.monitor.cvar.wait_timeout(lock, timeout).unwrap();

            lock = result.0;

            if result.1.timed_out() {
                break;
            } else {
                // In case of a spurious wakeup we want to sleep for the
                // _remainder_ of the time, not another full interval.
                // In practise this is unlikely to happen, but it's best
                // to handle it just in case.
                if let Some(remaining) =
                    self.interval.checked_sub(sleep_at.elapsed())
                {
                    timeout = remaining;
                } else {
                    break;
                }
            }
        }
    }

    fn deep_sleep(&self) {
        self.pool.monitor.status.store(MonitorStatus::Sleeping);

        let mut lock = self.pool.monitor.lock.lock().unwrap();

        // It's possible a thread entered blocking mode before we updated the
        // status and acquired the lock.
        if self.has_blocking_threads() {
            drop(lock);
            self.sleep();
            return;
        }

        // It's possible a thread notified us after we updated the status, but
        // before we acquired the lock. We can of course also be notified while
        // we're sleeping.
        while self.pool.monitor.status.load() == MonitorStatus::Sleeping
            && self.pool.is_alive()
        {
            lock = self.pool.monitor.cvar.wait(lock).unwrap();
        }

        self.pool.monitor.status.store(MonitorStatus::Normal);
    }

    fn has_blocking_threads(&self) -> bool {
        self.pool
            .threads
            .iter()
            .any(|t| t.blocked_at.load(Ordering::Acquire) != NOT_BLOCKING)
    }
}

struct MonitorState {
    /// The status of the monitor thread.
    status: AtomicCell<MonitorStatus>,

    /// The mutex used for putting a monitor to sleep.
    lock: Mutex<()>,

    /// A condition variable used for waking up the monitor thread.
    cvar: Condvar,
}

struct Pool {
    /// The shared state of each thread in this pool.
    threads: Vec<Shared>,

    /// A global queue to either schedule work on directly (e.g. when resuming a
    /// process from the network poller), or to overflow excess work into.
    ///
    /// We use a simple synchronised Vec with a large enough capacity to remove
    /// the need for resizing in common cases. We experimented with using
    /// crossbeam's SegQueue type, but didn't observe a statistically
    /// significant difference in performance. Besides that, the SegQueue type
    /// has some (potential) performance pitfalls:
    ///
    /// - https://github.com/crossbeam-rs/crossbeam/issues/398
    /// - https://github.com/crossbeam-rs/crossbeam/issues/794
    ///
    /// Another benefit of using a Vec is that we can quickly split it in half,
    /// without needing to perform many individual pop() calls.
    ///
    /// We don't use a VecDeque here because we pop half the values then push
    /// those into a thread's local queue, meaning the ordering isn't really
    /// relevant here.
    global: Mutex<Vec<ProcessPointer>>,

    /// A condition variable used for waking up sleeping threads.
    sleeping_cvar: Condvar,

    /// A flag indicating if the pool is alive and work should be consumed, or
    /// if we should shut down.
    alive: AtomicBool,

    /// The number of sleeping threads.
    ///
    /// This counter isn't necessarily 100% in sync with the actual amount of
    /// sleeping threads. This is OK because at worst we'll slightly delay
    /// waking up a thread, while at best we avoid acquiring a lock on the
    /// global queue.
    sleeping: AtomicU16,

    /// The epoch to use for tracking blocking operations.
    ///
    /// A separate thread increments this number at a fixed interval. If a
    /// thread is blocking and uses an older epoch, it signals the blocking
    /// operation may take a while and extra threads are needed.
    ///
    /// This counter wraps around upon overflowing. This is OK because even at
    /// an interval of one microsecond, it would take 584 542 years for this
    /// counter to overflow.
    ///
    /// This value starts at 1, and a value of zero is used to indicate a thread
    /// isn't blocking.
    epoch: AtomicU64,

    /// Threads that are blocking and should be replaced by a backup thread.
    blocked_threads: Mutex<VecDeque<usize>>,

    /// A condition variable used for waking up sleeping backup threads.
    blocked_cvar: Condvar,

    /// The state of the process monitor thread.
    monitor: MonitorState,
}

impl Pool {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn sleeping(&self) -> usize {
        self.sleeping.load(Ordering::Acquire) as usize
    }

    fn schedule(&self, process: ProcessPointer) {
        let mut queue = self.global.lock().unwrap();

        queue.push(process);

        if self.sleeping() > 0 {
            self.sleeping_cvar.notify_one();
        }
    }

    fn schedule_multiple(&self, mut processes: Vec<ProcessPointer>) {
        let mut queue = self.global.lock().unwrap();

        queue.append(&mut processes);

        if self.sleeping() > 0 {
            self.sleeping_cvar.notify_all();
        }
    }

    fn notify_one(&self) {
        // We need to acquire the lock so we don't signal a thread just before
        // it goes to sleep.
        let _lock = self.global.lock().unwrap();

        self.sleeping_cvar.notify_one();
    }

    fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        while let Some(proc) = self.global.lock().unwrap().pop() {
            Process::drop_and_deallocate(proc);
        }
    }
}

pub(crate) struct Scheduler {
    primary: usize,
    backup: usize,
    pool: ArcWithoutWeak<Pool>,
}

impl Scheduler {
    pub(crate) fn new(size: usize, backup: usize) -> Scheduler {
        let mut shared = Vec::with_capacity(size);

        for _ in 0..size {
            let queue =
                ArcWithoutWeak::new(ArrayQueue::new(LOCAL_QUEUE_CAPACITY));

            shared.push(Shared { queue, blocked_at: AtomicU64::new(0) });
        }

        let shared = ArcWithoutWeak::new(Pool {
            threads: shared,
            global: Mutex::new(Vec::with_capacity(GLOBAL_QUEUE_START_CAPACITY)),
            sleeping_cvar: Condvar::new(),
            alive: AtomicBool::new(true),
            sleeping: AtomicU16::new(0),
            epoch: AtomicU64::new(START_EPOCH),
            blocked_threads: Mutex::new(VecDeque::with_capacity(size + backup)),
            blocked_cvar: Condvar::new(),
            monitor: MonitorState {
                status: AtomicCell::new(MonitorStatus::Normal),
                lock: Mutex::new(()),
                cvar: Condvar::new(),
            },
        });

        Self { primary: size, backup, pool: shared }
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.pool.is_alive()
    }

    pub(crate) fn schedule_multiple(&self, processes: Vec<ProcessPointer>) {
        self.pool.schedule_multiple(processes);
    }

    pub(crate) fn terminate(&self) {
        let _gloabl = self.pool.global.lock().unwrap();
        let _blocked = self.pool.blocked_threads.lock().unwrap();
        let _monitor = self.pool.monitor.lock.lock().unwrap();

        self.pool.alive.store(false, Ordering::Release);
        self.pool.monitor.status.store(MonitorStatus::Notified);

        self.pool.sleeping_cvar.notify_all();
        self.pool.blocked_cvar.notify_all();
        self.pool.monitor.cvar.notify_one();
    }

    pub(crate) fn run(
        &self,
        state: &State,
        class: ClassPointer,
        method: MethodPointer,
    ) {
        let process = Process::main(class, method);
        let _ = scope(move |s| {
            s.builder()
                .name("proc monitor".to_string())
                .spawn(move |_| Monitor::new(&*self.pool).run())
                .unwrap();

            for id in 0..self.primary {
                s.builder()
                    .name(format!("proc {}", id))
                    .spawn(move |_| Thread::new(id, &*self.pool).run(state))
                    .unwrap();
            }

            for id in 0..self.backup {
                s.builder()
                    .name(format!("backup {}", id))
                    .spawn(move |_| Thread::backup(&*self.pool).run(state))
                    .unwrap();
            }

            self.pool.schedule(process);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Method;
    use crate::test::{
        empty_async_method, empty_process_class, new_main_process, new_process,
        setup,
    };
    use std::thread::sleep;

    #[test]
    fn test_thread_schedule() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1);
        let mut thread = Thread::new(0, &scheduler.pool);

        thread.schedule(process);

        assert_eq!(thread.work.len(), 1);
        assert!(scheduler.pool.global.lock().unwrap().is_empty());
    }

    #[test]
    fn test_thread_schedule_with_overflow() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1);
        let mut thread = Thread::new(0, &scheduler.pool);

        scheduler.pool.sleeping.fetch_add(1, Ordering::AcqRel);

        for _ in 0..LOCAL_QUEUE_CAPACITY {
            thread.schedule(process);
        }

        thread.schedule(process);

        assert_eq!(thread.work.len(), LOCAL_QUEUE_CAPACITY);
        assert_eq!(scheduler.pool.global.lock().unwrap().len(), 1);

        while thread.work.pop().is_some() {
            // Since we schedule the same process multiple times, we have to
            // ensure it doesn't also get dropped multiple times.
        }
    }

    #[test]
    fn test_thread_schedule_priority() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1);
        let mut thread = Thread::new(0, &scheduler.pool);

        thread.schedule_priority(process);

        assert_eq!(thread.priority, Some(process));
    }

    #[test]
    fn test_thread_run_with_local_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, &state.scheduler.pool);

        thread.schedule(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_with_priority_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, &state.scheduler.pool);

        thread.schedule_priority(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);
        assert!(thread.priority.is_none());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_with_stolen_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let mut thread0 = Thread::new(0, &state.scheduler.pool);
        let mut thread1 = Thread::new(1, &state.scheduler.pool);

        thread1.schedule(process);
        thread0.run(&state);

        assert_eq!(thread0.work.len(), 0);
        assert_eq!(thread1.work.len(), 0);

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_with_global_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, &state.scheduler.pool);

        state.scheduler.pool.schedule(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);
        assert!(state.scheduler.pool.global.lock().unwrap().is_empty());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_as_backup() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, &state.scheduler.pool);

        thread.backup = true;

        state.scheduler.pool.blocked_threads.lock().unwrap().push_back(1);
        thread.schedule(process);
        thread.run(&state);

        assert_eq!(thread.id, 1);
        assert!(!thread.backup);
        assert!(thread.work.is_empty());
        assert_eq!(
            thread.work.as_ptr(),
            state.scheduler.pool.threads[1].queue.as_ptr()
        );

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_as_backup_without_blocked_threads() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let state = setup();
        let pool = &state.scheduler.pool;

        // When a backup thread doesn't find any blocked threads it goes to
        // sleep. This test ensures it wakes up during termination.
        let _ = scope(|s| {
            s.spawn(|_| {
                let mut thread = Thread::new(0, pool);

                thread.backup = true;
                thread.schedule(process);
                thread.run(&state);
            });

            while pool.global.lock().unwrap().is_empty() {
                // Spin until the other thread moves its work to the global
                // queue.
                sleep(Duration::from_micros(10));
            }

            state.scheduler.terminate();
        });

        assert_eq!(pool.global.lock().unwrap().len(), 1);

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_start_blocking() {
        let class = empty_process_class("A");
        let proc1 = new_process(*class).take_and_forget();
        let proc2 = new_process(*class).take_and_forget();
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, pool);

        pool.epoch.store(4, Ordering::Release);
        pool.monitor.status.store(MonitorStatus::Sleeping);

        thread.schedule_priority(proc1);
        thread.schedule(proc2);
        thread.start_blocking();

        assert_eq!(thread.blocked_at, 4);
        assert_eq!(pool.threads[0].blocked_at.load(Ordering::Acquire), 4);
        assert_eq!(pool.monitor.status.load(), MonitorStatus::Notified);
        assert_eq!(thread.work.len(), 1);
        assert!(thread.priority.is_none());
    }

    #[test]
    fn test_thread_finish_blocking() {
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, pool);

        thread.start_blocking();
        thread.finish_blocking();

        assert!(!thread.backup);

        thread.start_blocking();
        pool.threads[0].blocked_at.store(NOT_BLOCKING, Ordering::Release);
        thread.finish_blocking();

        assert!(thread.backup);
        assert_eq!(thread.blocked_at, NOT_BLOCKING);
    }

    #[test]
    fn test_thread_blocking() {
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, pool);

        thread.blocking(|| {
            pool.threads[0].blocked_at.store(NOT_BLOCKING, Ordering::Release)
        });

        assert!(thread.backup);
    }

    #[test]
    fn test_thread_move_work_to_global_queue() {
        let class = empty_process_class("A");
        let proc1 = new_process(*class).take_and_forget();
        let proc2 = new_process(*class).take_and_forget();
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, pool);

        thread.schedule(proc1);
        thread.schedule_priority(proc2);
        thread.move_work_to_global_queue();

        assert!(thread.work.is_empty());
        assert!(thread.priority.is_none());
        assert_eq!(pool.global.lock().unwrap().len(), 2);
    }

    #[test]
    fn test_pool_schedule_with_sleeping_thread() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1);

        scheduler.pool.sleeping.fetch_add(1, Ordering::Release);
        scheduler.pool.schedule(process);

        assert_eq!(scheduler.pool.global.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_scheduler_terminate() {
        let scheduler = Scheduler::new(1, 1);
        let thread = Thread::new(0, &scheduler.pool);

        scheduler.pool.sleeping.fetch_add(1, Ordering::Release);
        scheduler.terminate();
        thread.sleep();

        assert!(!scheduler.is_alive());
    }

    #[test]
    fn test_monitor_status_is_lock_free() {
        assert!(AtomicCell::<MonitorStatus>::is_lock_free());
    }

    #[test]
    fn test_monitor_check_threads() {
        let scheduler = Scheduler::new(2, 2);
        let mut monitor = Monitor::new(&*scheduler.pool);

        assert!(!monitor.check_threads());

        // Epoch is the same, nothing needs to be done.
        scheduler.pool.threads[0].blocked_at.store(1, Ordering::Release);
        assert!(monitor.check_threads());
        assert_eq!(
            scheduler.pool.threads[0].blocked_at.load(Ordering::Acquire),
            1
        );

        // Epoch differs, the thread should be replaced.
        monitor.epoch = 2;
        assert!(monitor.check_threads());
        assert_eq!(
            scheduler.pool.threads[0].blocked_at.load(Ordering::Acquire),
            NOT_BLOCKING
        );
        assert_eq!(
            scheduler.pool.blocked_threads.lock().unwrap().pop_front(),
            Some(0)
        );
    }

    #[test]
    fn test_monitor_update_epoch() {
        let scheduler = Scheduler::new(1, 1);
        let mut monitor = Monitor::new(&*scheduler.pool);

        assert_eq!(monitor.epoch, START_EPOCH);
        assert_eq!(scheduler.pool.epoch.load(Ordering::Acquire), START_EPOCH);

        monitor.update_epoch();

        assert_eq!(monitor.epoch, 2);
        assert_eq!(scheduler.pool.epoch.load(Ordering::Acquire), 2);
    }

    #[test]
    fn test_monitor_sleep() {
        let scheduler = Scheduler::new(1, 1);
        let monitor = Monitor::new(&*scheduler.pool);
        let start = Instant::now();

        scheduler.pool.monitor.status.store(MonitorStatus::Notified);
        monitor.sleep();

        assert!(start.elapsed().as_micros() >= u128::from(MONITOR_INTERVAL));
        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }

    #[test]
    fn test_monitor_deep_sleep_with_termination() {
        let scheduler = Scheduler::new(1, 1);
        let monitor = Monitor::new(&*scheduler.pool);

        scheduler.terminate();
        monitor.deep_sleep();

        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }

    #[test]
    fn test_monitor_deep_sleep_with_notification() {
        let scheduler = Scheduler::new(1, 1);
        let monitor = Monitor::new(&*scheduler.pool);
        let _ = scope(|s| {
            s.spawn(|_| monitor.deep_sleep());

            while scheduler.pool.monitor.status.load()
                != MonitorStatus::Sleeping
            {
                sleep(Duration::from_micros(10));
            }

            let _lock = scheduler.pool.monitor.lock.lock().unwrap();

            scheduler.pool.monitor.status.store(MonitorStatus::Notified);
            scheduler.pool.monitor.cvar.notify_one();
        });

        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }

    #[test]
    fn test_monitor_deep_sleep_with_blocked_threads() {
        let scheduler = Scheduler::new(1, 1);
        let monitor = Monitor::new(&*scheduler.pool);

        scheduler.pool.threads[0].blocked_at.store(1, Ordering::Release);
        monitor.deep_sleep();

        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }
}
