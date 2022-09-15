//! Scheduling and execution of lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::machine::Machine;
use crate::mem::{ClassPointer, MethodPointer};
use crate::process::{Process, ProcessPointer};
use crate::scheduler::join_list::JoinList;
use crate::scheduler::park_group::ParkGroup;
use crate::scheduler::queue::{Queue, RcQueue};
use crate::state::RcState as VmState;
use crossbeam_deque::{Injector, Steal};
use std::iter;
use std::ops::Drop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// The internal state of a single pool, shared between the many threads that
/// belong to a pool.
pub(crate) struct State {
    /// The queues available for threads to store work in and steal work from.
    queues: Vec<RcQueue<ProcessPointer>>,

    /// A boolean indicating if the scheduler is alive, or should shut down.
    alive: AtomicBool,

    /// The global queue on which new jobs will be scheduled,
    global_queue: Injector<ProcessPointer>,

    /// Used for parking and unparking threads.
    park_group: ParkGroup,
}

impl State {
    /// Creates a new state for the given number threads.
    ///
    /// Threads are not started by this method, and instead must be started
    /// manually.
    fn new(threads: u16) -> Self {
        let queues =
            iter::repeat_with(Queue::with_rc).take(threads as usize).collect();

        State {
            alive: AtomicBool::new(true),
            queues,
            global_queue: Injector::new(),
            park_group: ParkGroup::new(),
        }
    }

    /// Schedules a new job onto the global queue.
    fn push_global(&self, value: ProcessPointer) {
        self.global_queue.push(value);
        self.park_group.notify_one();
    }

    /// Schedules a job onto a specific queue.
    ///
    /// This method will panic if the queue index is invalid.
    fn schedule_onto_queue(&self, queue: u16, value: ProcessPointer) {
        self.queues[queue as usize].push_external(value);

        // A thread might be parked when sending it an external message, so we
        // have to wake them up. We have to notify all threads instead of a
        // single one, otherwise we may end up notifying a different thread.
        self.park_group.notify_all();
    }

    /// Pops a value off the global queue.
    ///
    /// This method will block the calling thread until a value is available.
    pub(crate) fn pop_global(&self) -> Option<ProcessPointer> {
        loop {
            match self.global_queue.steal() {
                Steal::Empty => {
                    return None;
                }
                Steal::Retry => {}
                Steal::Success(value) => {
                    return Some(value);
                }
            }
        }
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub(crate) fn terminate(&self) {
        self.alive.store(false, Ordering::Release);
        self.notify_all();
    }

    pub(crate) fn notify_all(&self) {
        self.park_group.notify_all();
    }

    /// Parks the current thread as long as the given condition is true.
    pub(crate) fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        self.park_group.park_while(|| self.is_alive() && condition());
    }

    /// Returns true if one or more jobs are present in the global queue.
    pub(crate) fn has_global_jobs(&self) -> bool {
        !self.global_queue.is_empty()
    }
}

impl Drop for State {
    fn drop(&mut self) {
        while let Some(process) = self.global_queue.steal().success() {
            Process::drop_and_deallocate(process);
        }
    }
}

/// A pool of threads for running lightweight processes.
pub(crate) struct Pool {
    pub(crate) state: ArcWithoutWeak<State>,
}

impl Pool {
    pub(crate) fn new(threads: u16) -> Self {
        assert!(threads > 0, "A pool requires at least a single thread");

        Self { state: ArcWithoutWeak::new(State::new(threads)) }
    }

    /// Schedules a job onto a specific queue.
    pub(crate) fn schedule_onto_queue(&self, queue: u16, job: ProcessPointer) {
        self.state.schedule_onto_queue(queue, job);
    }

    /// Schedules a job onto the global queue.
    pub(crate) fn schedule(&self, job: ProcessPointer) {
        self.state.push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    pub(crate) fn terminate(&self) {
        self.state.terminate();
    }

    /// Starts the pool, blocking the current thread until the pool is
    /// terminated.
    ///
    /// The current thread will be used to perform jobs scheduled onto the first
    /// queue.
    pub(crate) fn start_main(
        &self,
        vm_state: VmState,
        class: ClassPointer,
        method: MethodPointer,
    ) -> JoinList<()> {
        let join_list = self.spawn_threads_for_range(1, vm_state.clone());
        let queue = self.state.queues[0].clone();
        let mut thread = Thread::main(queue, self.state.clone(), class, method);

        thread.run(&vm_state);
        join_list
    }

    /// Spawns OS threads for a range of queues, starting at the given position.
    fn spawn_threads_for_range(
        &self,
        start_at: usize,
        vm_state: VmState,
    ) -> JoinList<()> {
        let mut handles = Vec::new();

        for index in start_at..self.state.queues.len() {
            let handle = self.spawn_thread(
                vm_state.clone(),
                self.state.queues[index].clone(),
            );

            handles.push(handle);
        }

        JoinList::new(handles)
    }

    fn spawn_thread(
        &self,
        vm_state: VmState,
        queue: RcQueue<ProcessPointer>,
    ) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::spawn(move || {
            Thread::new(queue, state).run(&vm_state);
        })
    }
}

/// A scheduler for running Inko processes.
pub(crate) struct Scheduler {
    pub(crate) pool: Pool,
}

impl Scheduler {
    pub(crate) fn new(primary: u16) -> Self {
        Scheduler {
            // The primary pool gets one extra thread, as the main thread is
            // reserved for the main process. This makes interfacing with C
            // easier, as the main process is guaranteed to always run on the
            // same OS thread.
            pool: Pool::new(primary + 1),
        }
    }

    /// Informs the scheduler it needs to terminate as soon as possible.
    pub(crate) fn terminate(&self) {
        self.pool.terminate();
    }

    /// Schedules a process in one of the pools.
    pub(crate) fn schedule(&self, process: ProcessPointer) {
        if process.is_main() {
            self.pool.schedule_onto_queue(0, process);
        } else {
            self.pool.schedule(process);
        }
    }
}

/// A thread running Inko processes.
pub(crate) struct Thread {
    queue: RcQueue<ProcessPointer>,
    state: ArcWithoutWeak<State>,
}

impl Thread {
    fn main(
        queue: RcQueue<ProcessPointer>,
        state: ArcWithoutWeak<State>,
        class: ClassPointer,
        method: MethodPointer,
    ) -> Self {
        let process = Process::main(class, method);

        queue.push_internal(process);

        Thread { queue, state }
    }

    fn new(
        queue: RcQueue<ProcessPointer>,
        state: ArcWithoutWeak<State>,
    ) -> Self {
        Thread { queue, state }
    }

    pub(crate) fn run(&mut self, vm_state: &VmState) {
        while self.state.is_alive() {
            if self.process_local_jobs(vm_state)
                || self.steal_from_other_queue()
                || self.queue.move_external_jobs()
                || self.steal_from_global_queue()
            {
                continue;
            }

            self.state.park_while(|| {
                !self.state.has_global_jobs() && !self.queue.has_external_jobs()
            });
        }
    }

    /// Processes all local jobs until we run out of work.
    ///
    /// This method returns true if the thread should self terminate.
    fn process_local_jobs(&mut self, vm_state: &VmState) -> bool {
        loop {
            if !self.state.is_alive() {
                return true;
            }

            if let Some(job) = self.queue.pop() {
                Machine::new(vm_state).run(job);
            } else {
                return false;
            }
        }
    }

    fn steal_from_other_queue(&self) -> bool {
        // We may try to steal from our queue, but that's OK because it's empty
        // and none of the below operations are blocking.
        //
        // We don't steal from the first queue, because that's used for the main
        // thread/process.
        for queue in &self.state.queues[1..] {
            if queue.steal_into(&self.queue) {
                return true;
            }
        }

        false
    }

    /// Steals a single job from the global queue.
    ///
    /// This method will return `true` if a job was stolen.
    fn steal_from_global_queue(&self) -> bool {
        if let Some(job) = self.state.pop_global() {
            self.queue.push_internal(job);
            true
        } else {
            false
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        while let Some(process) = self.queue.pop() {
            Process::drop_and_deallocate(process);
        }

        while let Some(process) = self.queue.pop_external_job() {
            Process::drop_and_deallocate(process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use crate::mem::Method;
    use crate::test::{
        empty_async_method, empty_process_class, new_main_process, new_process,
        setup,
    };
    use std::mem;
    use std::thread;

    fn thread(state: &VmState) -> Thread {
        let pool_state = state.scheduler.pool.state.clone();
        let queue = pool_state.queues[0].clone();

        Thread::new(queue, pool_state)
    }

    #[test]
    fn test_scheduler_terminate() {
        let scheduler = Scheduler::new(1);

        scheduler.terminate();

        assert!(!scheduler.pool.state.is_alive());
    }

    #[test]
    fn test_schedule_on_primary() {
        let scheduler = Scheduler::new(1);
        let class = empty_process_class("A");
        let process = new_process(*class);
        let proc = *process;

        scheduler.schedule(proc);

        assert!(scheduler.pool.state.pop_global() == Some(proc));
    }

    #[test]
    fn test_run_global_jobs() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let proc_wrapper = new_main_process(*class, main_method);
        let process = proc_wrapper.take_and_forget();
        let mut thread = thread(&state);

        thread.state.push_global(process);
        thread.run(&state);

        assert!(thread.state.pop_global().is_none());
        assert!(thread.state.queues[0].pop().is_none());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_run_with_external_jobs() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let proc_wrapper = new_main_process(*class, main_method);
        let process = proc_wrapper.take_and_forget();
        let mut thread = thread(&state);

        thread.state.queues[0].push_external(process);
        thread.run(&state);

        assert!(!thread.state.queues[0].has_external_jobs());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_run_steal_then_terminate() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let proc_wrapper = new_main_process(*class, main_method);
        let process = proc_wrapper.take_and_forget();
        let mut thread = thread(&state);

        thread.state.queues[1].push_internal(process);
        thread.run(&state);

        assert!(thread.state.queues[1].pop().is_none());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_run_work_and_steal() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let proc_wrapper = new_main_process(*class, main_method);
        let mut thread = thread(&state);
        let process1 = proc_wrapper.take_and_forget();
        let process2 = new_process(*class);

        thread.queue.push_internal(*process2);
        thread.state.queues[1].push_internal(process1);

        // Here the order of work is:
        //
        // 1. Process local job
        // 2. Steal from other queue
        // 3. Terminate
        thread.run(&state);

        assert!(thread.queue.pop().is_none());
        assert!(thread.state.queues[1].pop().is_none());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_run_work_then_terminate_steal_loop() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let proc_wrapper = new_main_process(*class, main_method);
        let mut thread = thread(&state);
        let process1 = proc_wrapper.take_and_forget();
        let process2 = new_process(*class);

        thread.state.queues[0].push_internal(process1);
        thread.state.queues[1].push_internal(*process2);
        thread.run(&state);

        assert!(thread.state.queues[0].pop().is_none());
        assert!(thread.state.queues[1].pop().is_some());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    #[should_panic]
    fn test_new_pool_with_zero_threads() {
        Pool::new(0);
    }

    #[test]
    fn test_pool_terminate() {
        let state = setup();
        let pool = &state.scheduler.pool;

        assert!(pool.state.is_alive());
        pool.terminate();
        assert!(!pool.state.is_alive());
    }

    #[test]
    fn test_start_main() {
        let state = setup();
        let pool = &state.scheduler.pool;
        let class = empty_process_class("A");
        let method = empty_async_method();
        let threads = pool.start_main(state.clone(), *class, method);

        threads.join().unwrap();

        assert!(!pool.state.is_alive());

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_schedule_onto_queue() {
        let state = setup();
        let class = empty_process_class("A");
        let process = new_process(*class);
        let pool = &state.scheduler.pool;

        pool.schedule_onto_queue(0, *process);

        assert!(pool.state.queues[0].has_external_jobs());
    }

    #[test]
    fn test_spawn_thread() {
        let state = setup();
        let class = empty_process_class("A");
        let main_method = empty_async_method();
        let process = new_main_process(*class, main_method);
        let proc = process.take_and_forget();
        let pool = &state.scheduler.pool;
        let thread =
            pool.spawn_thread(state.clone(), pool.state.queues[0].clone());

        pool.schedule(proc);

        thread.join().unwrap();

        assert!(!pool.state.has_global_jobs());

        Method::drop_and_deallocate(main_method);
    }

    #[test]
    fn test_memory_size() {
        assert_eq!(mem::size_of::<State>(), 384);
    }

    #[test]
    fn test_new_pool_state() {
        let state: State = State::new(4);

        assert_eq!(state.queues.len(), 4);
    }

    #[test]
    fn test_push_global() {
        let class = empty_process_class("A");
        let proc_wrapper = new_process(*class);
        let process = proc_wrapper.take_and_forget();
        let state = State::new(1);

        state.push_global(process);

        assert!(!state.global_queue.is_empty());
    }

    #[test]
    fn test_pop_global() {
        let class = empty_process_class("A");
        let process = new_process(*class);
        let state = State::new(1);

        state.push_global(*process);

        assert!(state.pop_global().is_some());
        assert!(state.pop_global().is_none());
    }

    #[test]
    fn test_terminate() {
        let state: State = State::new(4);

        assert!(state.is_alive());

        state.terminate();

        assert!(!state.is_alive());
    }

    #[test]
    fn test_park_while() {
        let state: State = State::new(4);
        let mut number = 0;

        state.park_while(|| false);

        number += 1;

        state.terminate();
        state.park_while(|| true);

        number += 1;

        assert_eq!(number, 2);
    }

    #[test]
    fn test_has_global_jobs() {
        let class = empty_process_class("A");
        let proc_wrapper = new_process(*class);
        let process = proc_wrapper.take_and_forget();
        let state = State::new(4);

        assert!(!state.has_global_jobs());

        state.push_global(process);

        assert!(state.has_global_jobs());
    }

    #[test]
    fn test_pool_state_schedule_onto_queue() {
        let class = empty_process_class("A");
        let process = new_process(*class);
        let state = State::new(1);

        state.schedule_onto_queue(0, *process);

        assert!(state.queues[0].has_external_jobs());
    }

    #[test]
    #[should_panic]
    fn test_schedule_onto_invalid_queue() {
        let class = empty_process_class("A");
        let process = new_process(*class);
        let state = State::new(1);

        state.schedule_onto_queue(1, *process);
    }

    #[test]
    fn test_schedule_onto_queue_wake_up() {
        let class = empty_process_class("A");
        let process = new_process(*class);
        let state = ArcWithoutWeak::new(State::new(1));
        let state_clone = state.clone();

        state.schedule_onto_queue(0, *process);

        let handle = thread::spawn(move || {
            let queue = &state_clone.queues[0];

            state_clone.park_while(|| !queue.has_external_jobs());

            queue.pop_external_job()
        });

        let job = handle.join().unwrap();

        assert!(job.is_some());
    }
}
