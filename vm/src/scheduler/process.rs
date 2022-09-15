//! Scheduling and execution of lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::machine::Machine;
use crate::mem::{ClassPointer, MethodPointer};
use crate::process::{Process, ProcessPointer};
use crate::state::State;
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::sync::{Parker, Unparker};
use crossbeam_utils::thread::scope;
use std::cmp::min;
use std::mem::size_of;
use std::ops::Drop;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

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

/// The shared half of a thread.
struct Shared {
    /// The queue threads can steal work from.
    queue: ArcWithoutWeak<ArrayQueue<ProcessPointer>>,

    /// A handle used to unpark a sleeping thread.
    unparker: Unparker,
}

/// The private half of a thread, used only by the OS thread this state belongs
/// to.
pub(crate) struct Thread {
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

    /// A handle used for parking this thread.
    parker: Parker,
}

impl Thread {
    fn new(id: usize, parker: Parker, shared: ArcWithoutWeak<Pool>) -> Thread {
        let work = shared.threads[id].queue.clone();

        Self { id, work, priority: None, pool: shared, parker }
    }

    pub(crate) fn schedule(&mut self, process: ProcessPointer) {
        if let Err(process) = self.work.push(process) {
            self.pool.schedule(process);
            return;
        }

        if self.work.len() > 1 && self.pool.sleeping() > 0 {
            self.pool.unpark_one();
        }
    }

    pub(crate) fn schedule_priority(&mut self, process: ProcessPointer) {
        // Outside of any bugs in the VM, we should never reach this point and
        // still have a value in the priority slot, so we can just set the value
        // as-is.
        self.priority = Some(process);
    }

    /// Starts the run loop of this thread.
    ///
    /// This method is marked as `unsafe` because it's only safe to call it from
    /// a single owning thread.
    fn run(&mut self, state: &State) {
        while self.pool.is_alive() {
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
        self.transition_to_sleeping();

        // When another thread signals termination of the pool, it does so
        // before attempting to wake up any sleeping threads. If we don't check
        // the pool state again here, we may end up sleeping without ever being
        // woken up.
        if self.pool.is_alive() {
            self.parker.park();
        }

        // At this point we were either woken up by another worker because new
        // work is produced, or because we're shutting down. In neither case do
        // we need to remove our ID from the sleepers list, because it's either
        // already done or redundant. Spurious wake-ups are not a problem, as
        // crossbeam's `Parker::park()` handles this for us.
        self.pool.sleeping.fetch_sub(1, Ordering::AcqRel);
    }

    fn transition_to_sleeping(&self) {
        self.pool.sleepers.lock().unwrap().push(self.id);
        self.pool.sleeping.fetch_add(1, Ordering::AcqRel);
    }

    fn run_process(&mut self, state: &State, process: ProcessPointer) {
        Machine::new(state).run(self, process);
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        while let Some(process) = self.work.pop() {
            Process::drop_and_deallocate(process);
        }

        if let Some(process) = self.priority.take() {
            Process::drop_and_deallocate(process);
        }
    }
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

    /// A flag indicating if the pool is alive and work should be consumed, or
    /// if we should shut down.
    alive: AtomicBool,

    /// The IDs of th threads that are asleep.
    sleepers: Mutex<Vec<usize>>,

    /// The number of sleeping threads.
    ///
    /// This counter isn't necessarily 100% in sync with the actual list of
    /// sleepers. This is OK because at worst we'll slightly delay waking up a
    /// thread, while at best we avoid acquiring a lock on an empty sleeper
    /// list.
    sleeping: AtomicUsize,
}

impl Pool {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn sleeping(&self) -> usize {
        self.sleeping.load(Ordering::Acquire)
    }

    fn schedule(&self, process: ProcessPointer) {
        self.global.lock().unwrap().push(process);

        if self.sleeping() > 0 {
            self.unpark_one();
        }
    }

    fn unpark_one(&self) {
        if let Some(id) = self.sleepers.lock().unwrap().pop() {
            self.threads[id].unparker.unpark();
        }
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
    pool: ArcWithoutWeak<Pool>,
}

impl Scheduler {
    pub(crate) fn new(size: usize) -> (Scheduler, Vec<Thread>) {
        assert!(size > 0, "A pool requires at least a single thread");

        let mut shared = Vec::with_capacity(size);
        let mut parkers = Vec::with_capacity(size);

        for _ in 0..size {
            let queue =
                ArcWithoutWeak::new(ArrayQueue::new(LOCAL_QUEUE_CAPACITY));
            let parker = Parker::new();
            let unparker = parker.unparker().clone();

            parkers.push(parker);
            shared.push(Shared { queue, unparker });
        }

        let shared = ArcWithoutWeak::new(Pool {
            threads: shared,
            global: Mutex::new(Vec::with_capacity(GLOBAL_QUEUE_START_CAPACITY)),
            alive: AtomicBool::new(true),
            sleepers: Mutex::new(Vec::new()),
            sleeping: AtomicUsize::new(0),
        });

        let threads = parkers
            .into_iter()
            .enumerate()
            .map(|(id, parker)| Thread::new(id, parker, shared.clone()))
            .collect();

        (Self { pool: shared }, threads)
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.pool.is_alive()
    }

    pub(crate) fn schedule(&self, process: ProcessPointer) {
        self.pool.schedule(process);
    }

    pub(crate) fn terminate(&self) {
        self.pool.alive.store(false, Ordering::Release);

        for &id in self.pool.sleepers.lock().unwrap().iter() {
            // During shutdown we don't care about leaving behind sleeping IDs,
            // so we just leave the list as-is.
            self.pool.threads[id].unparker.unpark();
        }
    }

    pub(crate) fn run(
        &self,
        state: &State,
        class: ClassPointer,
        method: MethodPointer,
        threads: Vec<Thread>,
    ) {
        let process = Process::main(class, method);
        let _ = threads[0].work.push(process);
        let _ = scope(move |s| {
            for mut thread in threads {
                s.builder()
                    .name(format!("proc {}", thread.id))
                    .spawn(move |_| {
                        thread.run(state);
                    })
                    .unwrap();
            }
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

    #[test]
    fn test_thread_schedule() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let (scheduler, mut threads) = Scheduler::new(1);
        let thread = &mut threads[0];

        thread.schedule(process);

        assert_eq!(thread.work.len(), 1);
        assert!(scheduler.pool.global.lock().unwrap().is_empty());
    }

    #[test]
    fn test_thread_schedule_with_overflow() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let (scheduler, mut threads) = Scheduler::new(1);
        let thread = &mut threads[0];

        thread.transition_to_sleeping();

        for _ in 0..LOCAL_QUEUE_CAPACITY {
            thread.schedule(process);
        }

        thread.schedule(process);

        assert_eq!(thread.work.len(), LOCAL_QUEUE_CAPACITY);
        assert_eq!(scheduler.pool.global.lock().unwrap().len(), 1);
        assert!(scheduler.pool.sleepers.lock().unwrap().is_empty());

        while thread.work.pop().is_some() {
            // Since we schedule the same process multiple times, we have to
            // ensure it doesn't also get dropped multiple times.
        }
    }

    #[test]
    fn test_thread_schedule_priority() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let (_, mut threads) = Scheduler::new(1);
        let thread = &mut threads[0];

        thread.schedule_priority(process);

        assert_eq!(thread.priority, Some(process));
    }

    #[test]
    fn test_thread_run_with_local_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let (state, mut threads) = setup();
        let thread = &mut threads[0];

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
        let (state, mut threads) = setup();
        let thread = &mut threads[0];

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
        let (state, mut threads) = setup();

        threads[1].schedule(process);
        threads[0].run(&state);

        assert_eq!(threads[0].work.len(), 0);
        assert_eq!(threads[1].work.len(), 0);

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_thread_run_with_global_job() {
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method).take_and_forget();
        let (state, mut threads) = setup();
        let thread = &mut threads[0];

        state.scheduler.schedule(process);
        thread.run(&state);

        assert_eq!(thread.work.len(), 0);
        assert!(state.scheduler.pool.global.lock().unwrap().is_empty());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_pool_schedule() {
        let class = empty_process_class("A");
        let process = new_process(*class).take_and_forget();
        let scheduler = Scheduler::new(1).0;

        scheduler.pool.sleepers.lock().unwrap().push(0);
        scheduler.pool.sleeping.fetch_add(1, Ordering::Release);
        scheduler.schedule(process);

        assert_eq!(scheduler.pool.global.lock().unwrap().len(), 1);
        assert!(scheduler.pool.sleepers.lock().unwrap().is_empty());
    }

    #[test]
    fn test_scheduler_terminate() {
        let (scheduler, threads) = Scheduler::new(1);

        scheduler.pool.sleepers.lock().unwrap().push(0);
        scheduler.pool.sleeping.fetch_add(1, Ordering::Release);
        scheduler.terminate();

        // If we didn't call unpark, we'd hang here forever.
        threads[0].parker.park();

        assert!(!scheduler.is_alive());
    }
}
