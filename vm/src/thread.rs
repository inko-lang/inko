//! Virtual Machine Threads

use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::time::Duration;

use process::RcProcess;

pub type RcThread = Arc<Thread>;
pub type JoinHandle = thread::JoinHandle<()>;

pub struct Thread {
    pub process_queue: Mutex<Vec<RcProcess>>,
    pub wakeup_signaler: Condvar,
    pub should_stop: Mutex<bool>,
    pub join_handle: Mutex<Option<JoinHandle>>,
}

impl Thread {
    pub fn new(handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            process_queue: Mutex::new(Vec::new()),
            wakeup_signaler: Condvar::new(),
            should_stop: Mutex::new(false),
            join_handle: Mutex::new(handle),
        };

        Arc::new(thread)
    }

    pub fn stop(&self) {
        let mut stop = lock!(self.should_stop);

        *stop = true;

        self.wakeup_signaler.notify_all();
    }

    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        lock!(self.join_handle).take()
    }

    pub fn should_stop(&self) -> bool {
        *lock!(self.should_stop)
    }

    pub fn process_queue_size(&self) -> usize {
        lock!(self.process_queue).len()
    }

    pub fn process_queue_empty(&self) -> bool {
        self.process_queue_size() == 0
    }

    pub fn schedule(&self, process: RcProcess) {
        let mut queue = lock!(self.process_queue);

        queue.push(process.clone());

        self.wakeup_signaler.notify_all();
    }

    pub fn reschedule(&self, process: RcProcess) {
        process.reset_status();
        self.schedule(process);
    }

    pub fn pop_process(&self) -> Option<RcProcess> {
        let mut queue = lock!(self.process_queue);
        let timeout = Duration::from_millis(5);

        while queue.len() == 0 {
            if self.should_stop() {
                return None;
            }

            let (new_queue, _) =
                self.wakeup_signaler.wait_timeout(queue, timeout).unwrap();

            queue = new_queue;
        }

        queue.pop()
    }
}
