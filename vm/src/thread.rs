//! Virtual Machine Threads

use std::sync::{Arc, Mutex, Condvar};
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use process::RcProcess;

pub type RcThread = Arc<Thread>;
pub type JoinHandle = thread::JoinHandle<()>;

pub struct Thread {
    pub process_queue: Mutex<Vec<RcProcess>>,
    pub remembered_processes: Mutex<HashSet<RcProcess>>,
    pub wakeup_signaler: Condvar,
    pub should_stop: Mutex<bool>,
    pub join_handle: Mutex<Option<JoinHandle>>,
    pub main_thread: bool,
}

impl Thread {
    pub fn new(main_thread: bool, handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            process_queue: Mutex::new(Vec::new()),
            remembered_processes: Mutex::new(HashSet::new()),
            wakeup_signaler: Condvar::new(),
            should_stop: Mutex::new(false),
            join_handle: Mutex::new(handle),
            main_thread: main_thread,
        };

        Arc::new(thread)
    }

    pub fn stop(&self) {
        let mut stop = unlock!(self.should_stop);

        *stop = true;

        self.wakeup_signaler.notify_all();
    }

    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        unlock!(self.join_handle).take()
    }

    pub fn should_stop(&self) -> bool {
        *unlock!(self.should_stop)
    }

    pub fn process_queue_size(&self) -> usize {
        unlock!(self.process_queue).len()
    }

    pub fn process_queue_empty(&self) -> bool {
        self.process_queue_size() == 0
    }

    pub fn has_remembered_processes(&self) -> bool {
        unlock!(self.remembered_processes).len() > 0
    }

    pub fn main_can_terminate(&self) -> bool {
        self.main_thread && self.process_queue_empty() &&
        !self.has_remembered_processes()
    }

    pub fn schedule(&self, process: RcProcess) {
        let mut queue = unlock!(self.process_queue);

        queue.push(process.clone());

        self.wakeup_signaler.notify_all();
    }

    pub fn reschedule(&self, process: RcProcess) {
        unlock!(self.remembered_processes).remove(&process);

        process.reset_status();
        self.schedule(process);
    }

    pub fn remember_process(&self, process: RcProcess) {
        unlock!(self.remembered_processes).insert(process);
    }

    pub fn wait_for_work(&self) {
        let mut queue = unlock!(self.process_queue);
        let timeout = Duration::from_millis(5);

        while queue.len() == 0 {
            if self.should_stop() {
                return;
            }

            let (new_queue, _) =
                self.wakeup_signaler.wait_timeout(queue, timeout).unwrap();

            queue = new_queue;
        }
    }

    pub fn pop_process(&self) -> Option<RcProcess> {
        let mut queue = unlock!(self.process_queue);

        queue.pop()
    }
}
