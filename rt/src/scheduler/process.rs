//! Scheduling and execution of lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::context;
use crate::process::{Process, ProcessPointer, Task};
use crate::scheduler::{number_of_cores, pin_thread_to_core};
use crate::stack::StackPool;
use crate::state::State;
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::atomic::AtomicCell;
use crossbeam_utils::thread::scope;
use rand::rngs::ThreadRng;
use rand::thread_rng;
use std::cmp::min;
use std::collections::VecDeque;
use std::mem::{size_of, swap};
use std::ops::Drop;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The interval to wait (in milliseconds) between scheduling epoch updates.
///
/// This is the interval at which the epoch thread wakes up. The time for which
/// a process is allowed to run is likely a little higher than this.
const EPOCH_INTERVAL: u64 = 10;

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

pub(crate) fn epoch_loop(state: &State) {
    while state.scheduler.pool.is_alive() {
        sleep(Duration::from_millis(EPOCH_INTERVAL));
        state.scheduler_epoch.fetch_add(1, Ordering::Relaxed);
    }
}

/// A type describing what a thread should do in response to a process yielding
/// back control.
///
/// This type exists as processes may need to perform certain operations that
/// aren't safe to perform while the process is still running. For example, a
/// process may want to send a message to a receiver and wait for the result. If
/// the receiver is still running, it may end up trying to reschedule the
/// sender. If this happens while the sender is still wrapping up, all sorts of
/// things can go wrong.
///
/// To prevent such problems, processes yield control back to the thread and let
/// the thread perform such operations using its own stack.
#[derive(Debug)]
pub(crate) enum Action {
    /// The thread shouldn't do anything with the process it was running.
    Ignore,

    /// The thread should terminate the process.
    Terminate,
}

impl Action {
    fn take(&mut self) -> Self {
        let mut old_val = Action::Ignore;

        swap(self, &mut old_val);
        old_val
    }
}

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
pub struct Thread {
    /// The unique ID of this thread.
    ///
    /// This is used to prevent a thread from trying to steal work from itself,
    /// which is redundant.
    id: usize,

    /// The thread-local queue new work is scheduled onto, unless we consider it
    /// to be too full.
    work: ArcWithoutWeak<ArrayQueue<ProcessPointer>>,

    /// The pool this thread belongs to.
    pool: ArcWithoutWeak<Pool>,

    /// A flag indicating this thread is or will become a backup thread.
    backup: bool,

    /// The epoch at which we started blocking.
    ///
    /// This value mirrors `Shared.blocked_at` and is used to detect if a
    /// monitor thread changed the status.
    ///
    /// A value of 0 indicates the thread isn't blocked.
    blocked_at: u64,

    /// The ID of the network poller assigned to this thread.
    ///
    /// Threads are each assigned a network poller in a round-robin fashion.
    /// This is useful for programs that heavily rely on sockets, as a single
    /// network poller thread may not be able to complete its work fast enough.
    pub(crate) network_poller: usize,

    /// A random number generator to use for the current thread.
    pub(crate) rng: ThreadRng,

    /// The pool of stacks to use.
    pub(crate) stacks: StackPool,

    /// A value indicating what to do with a process when it yields back to us.
    ///
    /// The default is to not do anything with a process after it yields back to
    /// the thread.
    pub(crate) action: Action,
}

impl Thread {
    fn new(
        id: usize,
        network_poller: usize,
        pool: ArcWithoutWeak<Pool>,
    ) -> Thread {
        Self {
            id,
            work: pool.threads[id].queue.clone(),
            backup: false,
            blocked_at: NOT_BLOCKING,
            network_poller,
            rng: thread_rng(),
            stacks: StackPool::new(pool.stack_size),
            action: Action::Ignore,
            pool,
        }
    }

    fn backup(network_poller: usize, pool: ArcWithoutWeak<Pool>) -> Thread {
        Self {
            // For backup threads the ID/queue doesn't matter, because we won't
            // use them until we're turned into a regular thread.
            id: 0,
            work: pool.threads[0].queue.clone(),
            backup: true,
            blocked_at: NOT_BLOCKING,
            network_poller,
            rng: thread_rng(),
            stacks: StackPool::new(pool.stack_size),
            action: Action::Ignore,
            pool,
        }
    }

    /// Schedules a process onto the local queue, overflowing to the global
    /// queue if the local queue is full.
    ///
    /// This method shouldn't be used when the thread is to transition to a
    /// backup thread, as the work might never get picked up again.
    pub(crate) fn schedule(&mut self, process: ProcessPointer) {
        if let Err(process) = self.work.push(process) {
            self.pool.schedule(process);
            return;
        }

        if self.work.len() > 1 && self.pool.sleeping() > 0 {
            self.pool.notify_one();
        }
    }

    /// Schedules a process onto the global queue.
    pub(crate) fn schedule_global(&self, process: ProcessPointer) {
        self.pool.schedule(process);
    }

    pub(crate) fn start_blocking(&mut self) {
        // We have to push our work away before entering the blocking operation.
        // If we do this after returning from the blocking operation, another
        // thread may already be using our queue, at which point we'd slow it
        // down by moving its work to the global queue.
        self.move_work_to_global_queue();

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
        }

        self.blocked_at = NOT_BLOCKING;
    }

    pub(crate) fn blocking<F, R>(
        &mut self,
        process: ProcessPointer,
        function: F,
    ) -> R
    where
        F: FnOnce() -> R,
    {
        self.start_blocking();

        let res = function();

        self.finish_blocking();

        // If the closure took too long to run (e.g. an IO operation took too
        // long), we have to give up running the process. If we continue running
        // we could mess up whatever thread has taken over our queue/work, and
        // we'd be using the OS thread even longer than we already have.
        //
        // We schedule onto the global queue because if another thread took over
        // but found no other work, it may have gone to sleep. In that case
        // scheduling onto the local queue may result in the work never getting
        // picked up (e.g. if all other threads are also sleeping).
        if self.backup {
            // Safety: the current thread is holding on to the run lock, so
            // another thread can't run the process until we finish the context
            // switch.
            self.schedule_global(process);
            unsafe { context::switch(process) };
        }

        res
    }

    fn move_work_to_global_queue(&mut self) {
        let len = self.work.len();

        if len == 0 {
            return;
        }

        let mut work = Vec::with_capacity(len);

        while let Some(process) = self.work.pop() {
            work.push(process);
        }

        self.pool.schedule_multiple(work);
    }

    fn run(&mut self, state: &State) {
        while self.pool.is_alive() {
            if self.backup {
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

            // Now that we ran out of local work, we can try to shrink the stack
            // if really necessary. We do this _before_ stealing global work to
            // prevent the stack pool from ballooning in size. If we did this
            // before going to sleep then in an active system we may never end
            // up shrinking the stack pool.
            self.stacks.shrink();

            if let Some(process) = self.steal_from_global() {
                self.run_process(state, process);
                continue;
            }

            self.sleep();
        }
    }

    fn next_local_process(&mut self) -> Option<ProcessPointer> {
        self.work.pop()
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
                let mut to_steal = global.split_off(steal - 1);

                drop(global);

                while let Some(process) = to_steal.pop() {
                    if let Err(process) = self.work.push(process) {
                        to_steal.push(process);
                        self.pool.schedule_multiple(to_steal);
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

    /// Runs a process by calling back into the native code.
    fn run_process(&mut self, state: &State, mut process: ProcessPointer) {
        {
            // We must acquire the run lock first to prevent running a process
            // that's still wrapping up/suspending in another thread.
            //
            // An example of such a scenario is when process A sends a message
            // to process B, wants to wait for it, but B produces the result and
            // tries to reschedule A _before_ A gets a chance to finish yielding
            // back to the scheduler.
            //
            // This is done in a sub scope such that the lock is unlocked
            // automatically when we decide what action to take in response to
            // the yield.
            let _lock = process.acquire_run_lock();

            match process.next_task() {
                Task::Resume => {
                    process.resume(state, self);
                    unsafe { context::switch(process) }
                }
                Task::Start(func, args) => {
                    process.resume(state, self);
                    unsafe { context::start(state, process, func, args) }
                }
                Task::Wait => return,
            }

            process.unset_thread();
        }

        match self.action.take() {
            Action::Terminate => {
                // Process termination can't be safely done on the process'
                // stack, because its memory would be dropped while we're still
                // using it, hence we do that here.
                if process.is_main() {
                    state.terminate();
                }

                if let Some(stack) = process.take_stack() {
                    self.stacks.add(stack);
                }

                // Processes drop/free themselves as this must be deferred until
                // all messages (including any destructors) have finished
                // running. If we did this in a destructor we'd end up releasing
                // memory of a process while still using it.
                Process::drop_and_deallocate(process);
            }
            Action::Ignore => {
                // In this case it's up to the process (or another process) to
                // reschedule the process we just finished running.
            }
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        while let Some(process) = self.work.pop() {
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

    /// The size of each stack to allocate for a process.
    stack_size: usize,
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
        if processes.is_empty() {
            return;
        }

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
    pub(crate) fn new(
        size: usize,
        backup: usize,
        stack_size: usize,
    ) -> Scheduler {
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
            stack_size,
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
        let _global = self.pool.global.lock().unwrap();
        let _blocked = self.pool.blocked_threads.lock().unwrap();
        let _monitor = self.pool.monitor.lock.lock().unwrap();

        self.pool.alive.store(false, Ordering::Release);
        self.pool.monitor.status.store(MonitorStatus::Notified);

        self.pool.sleeping_cvar.notify_all();
        self.pool.blocked_cvar.notify_all();
        self.pool.monitor.cvar.notify_one();
    }

    pub(crate) fn run(&self, state: &State, process: ProcessPointer) {
        let pollers = state.network_pollers.len();
        let cores = number_of_cores();
        let _ = scope(move |s| {
            s.builder()
                .name("proc monitor".to_string())
                .spawn(move |_| {
                    // Cores 0 and 1 are used for the timeout and network poller
                    // threads. Since we may be running quite often we'll pin
                    // this thread to a different core.
                    pin_thread_to_core(2 % cores);
                    Monitor::new(&self.pool).run()
                })
                .unwrap();

            s.builder()
                .name("epoch".to_string())
                .spawn(move |_| {
                    pin_thread_to_core(3 % cores);
                    epoch_loop(state);
                })
                .unwrap();

            for id in 0..self.primary {
                let poll_id = id % pollers;

                s.builder()
                    .name(format!("proc {}", id))
                    .spawn(move |_| {
                        pin_thread_to_core(id % cores);
                        Thread::new(id, poll_id, self.pool.clone()).run(state)
                    })
                    .unwrap();
            }

            for id in 0..self.backup {
                let poll_id = id % pollers;

                s.builder()
                    .name(format!("backup {}", id))
                    .spawn(move |_| {
                        pin_thread_to_core(id % cores);
                        Thread::backup(poll_id, self.pool.clone()).run(state)
                    })
                    .unwrap();
            }

            self.pool.schedule(process);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::test::{
        empty_process_class, new_main_process, new_process, setup,
    };
    use std::thread::sleep;

    unsafe extern "system" fn method(ctx: *mut u8) {
        let ctx = &mut *(ctx as *mut Context);

        ctx.process.thread().action = Action::Terminate;
        context::switch(ctx.process);
    }

    #[test]
    fn test_thread_schedule() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1, 32);
        let mut thread = Thread::new(0, 0, scheduler.pool.clone());

        thread.schedule(process);

        assert_eq!(thread.work.len(), 1);
        assert!(scheduler.pool.global.lock().unwrap().is_empty());
    }

    #[test]
    fn test_thread_schedule_with_overflow() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1, 32);
        let mut thread = Thread::new(0, 0, scheduler.pool.clone());

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
    fn test_thread_run_with_local_job() {
        let class = empty_process_class("A");
        let process = new_main_process(*class, method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, 0, state.scheduler.pool.clone());

        thread.schedule(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);
    }

    #[test]
    fn test_thread_run_with_stolen_job() {
        let class = empty_process_class("A");
        let process = new_main_process(*class, method).take_and_forget();
        let state = setup();
        let mut thread0 = Thread::new(0, 0, state.scheduler.pool.clone());
        let mut thread1 = Thread::new(1, 0, state.scheduler.pool.clone());

        thread1.schedule(process);
        thread0.run(&state);

        assert_eq!(thread0.work.len(), 0);
        assert_eq!(thread1.work.len(), 0);
    }

    #[test]
    fn test_thread_run_with_global_job() {
        let class = empty_process_class("A");
        let process = new_main_process(*class, method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, 0, state.scheduler.pool.clone());

        state.scheduler.pool.schedule(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);
        assert!(state.scheduler.pool.global.lock().unwrap().is_empty());
    }

    #[test]
    fn test_thread_steal_from_global_with_full_local_queue() {
        let class = empty_process_class("A");
        let process = new_main_process(*class, method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, 0, state.scheduler.pool.clone());

        for _ in 0..LOCAL_QUEUE_CAPACITY {
            thread.schedule(process);
        }

        state.scheduler.pool.schedule(process);
        state.scheduler.pool.schedule(process);
        state.scheduler.pool.schedule(process);
        state.scheduler.pool.schedule(process);

        // When the scheduler/threads are dropped, pending processes are
        // deallocated. Since we're pushing the same process many times, we have
        // to clear the queues first before setting any assertions that may
        // fail.
        let stolen = thread.steal_from_global().is_some();
        let global_len = state.scheduler.pool.global.lock().unwrap().len();

        state.scheduler.pool.global.lock().unwrap().clear();

        for _ in 0..LOCAL_QUEUE_CAPACITY {
            thread.work.pop();
        }

        assert!(stolen);
        assert_eq!(global_len, 3);
    }

    #[test]
    fn test_thread_run_as_backup() {
        let class = empty_process_class("A");
        let process = new_main_process(*class, method).take_and_forget();
        let state = setup();
        let mut thread = Thread::new(0, 0, state.scheduler.pool.clone());

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
    }

    #[test]
    fn test_thread_start_blocking() {
        let class = empty_process_class("A");
        let proc = new_process(*class).take_and_forget();
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, 0, pool.clone());

        pool.epoch.store(4, Ordering::Release);
        pool.monitor.status.store(MonitorStatus::Sleeping);

        thread.schedule(proc);
        thread.start_blocking();

        assert_eq!(thread.blocked_at, 4);
        assert_eq!(pool.threads[0].blocked_at.load(Ordering::Acquire), 4);
        assert_eq!(pool.monitor.status.load(), MonitorStatus::Notified);
        assert!(thread.work.is_empty());
    }

    #[test]
    fn test_thread_finish_blocking() {
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, 0, pool.clone());

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
    fn test_thread_move_work_to_global_queue() {
        let class = empty_process_class("A");
        let proc = new_process(*class).take_and_forget();
        let state = setup();
        let pool = &state.scheduler.pool;
        let mut thread = Thread::new(0, 0, pool.clone());

        thread.schedule(proc);
        thread.move_work_to_global_queue();

        assert!(thread.work.is_empty());
        assert_eq!(pool.global.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_pool_schedule_with_sleeping_thread() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1, 1, 32);

        scheduler.pool.sleeping.fetch_add(1, Ordering::Release);
        scheduler.pool.schedule(process);

        assert_eq!(scheduler.pool.global.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_scheduler_terminate() {
        let scheduler = Scheduler::new(1, 1, 32);
        let thread = Thread::new(0, 0, scheduler.pool.clone());

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
        let scheduler = Scheduler::new(2, 2, 32);
        let mut monitor = Monitor::new(&scheduler.pool);

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
        let scheduler = Scheduler::new(1, 1, 32);
        let mut monitor = Monitor::new(&scheduler.pool);

        assert_eq!(monitor.epoch, START_EPOCH);
        assert_eq!(scheduler.pool.epoch.load(Ordering::Acquire), START_EPOCH);

        monitor.update_epoch();

        assert_eq!(monitor.epoch, 2);
        assert_eq!(scheduler.pool.epoch.load(Ordering::Acquire), 2);
    }

    #[test]
    fn test_monitor_sleep() {
        let scheduler = Scheduler::new(1, 1, 32);
        let monitor = Monitor::new(&scheduler.pool);
        let start = Instant::now();

        scheduler.pool.monitor.status.store(MonitorStatus::Notified);
        monitor.sleep();

        assert!(start.elapsed().as_micros() >= u128::from(MONITOR_INTERVAL));
        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }

    #[test]
    fn test_monitor_deep_sleep_with_termination() {
        let scheduler = Scheduler::new(1, 1, 32);
        let monitor = Monitor::new(&scheduler.pool);

        scheduler.terminate();
        monitor.deep_sleep();

        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }

    #[test]
    fn test_monitor_deep_sleep_with_notification() {
        let scheduler = Scheduler::new(1, 1, 32);
        let monitor = Monitor::new(&scheduler.pool);
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
        let scheduler = Scheduler::new(1, 1, 32);
        let monitor = Monitor::new(&scheduler.pool);

        scheduler.pool.threads[0].blocked_at.store(1, Ordering::Release);
        monitor.deep_sleep();

        assert_eq!(scheduler.pool.monitor.status.load(), MonitorStatus::Normal);
    }
}
