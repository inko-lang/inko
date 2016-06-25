//! Virtual Machine Threads

use std::sync::{Arc, Mutex, Condvar};
use std::thread;

use process::RcProcess;

pub type RcThread = Arc<Thread>;
pub type JoinHandle = thread::JoinHandle<()>;

pub struct Thread {
    pub process_queue: Mutex<Vec<RcProcess>>,
    pub wake_up: Mutex<bool>,
    pub wakeup_signaler: Condvar,
    pub should_stop: Mutex<bool>,
    pub join_handle: Mutex<Option<JoinHandle>>,
    pub main_thread: bool,
}

impl Thread {
    pub fn new(main_thread: bool, handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            process_queue: Mutex::new(Vec::new()),
            wake_up: Mutex::new(false),
            wakeup_signaler: Condvar::new(),
            should_stop: Mutex::new(false),
            join_handle: Mutex::new(handle),
            main_thread: main_thread,
        };

        Arc::new(thread)
    }

    pub fn stop(&self) {
        let mut stop = self.should_stop.lock().unwrap();
        let mut wake_up = self.wake_up.lock().unwrap();

        *stop = true;
        *wake_up = true;

        self.wakeup_signaler.notify_all();
    }

    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        self.join_handle.lock().unwrap().take()
    }

    pub fn should_stop(&self) -> bool {
        *self.should_stop.lock().unwrap()
    }

    pub fn process_queue_size(&self) -> usize {
        self.process_queue.lock().unwrap().len()
    }

    pub fn process_queue_empty(&self) -> bool {
        self.process_queue_size() == 0
    }

    pub fn schedule(&self, task: RcProcess) {
        let mut queue = self.process_queue.lock().unwrap();
        let mut wake_up = self.wake_up.lock().unwrap();

        queue.push(task);
        *wake_up = true;

        self.wakeup_signaler.notify_all();
    }

    pub fn wait_for_work(&self) {
        if self.process_queue_empty() {
            let mut wake_up = self.wake_up.lock().unwrap();

            while !*wake_up {
                wake_up = self.wakeup_signaler.wait(wake_up).unwrap();
            }

            *wake_up = false;
        }
    }

    pub fn pop_process(&self) -> RcProcess {
        let mut queue = self.process_queue.lock().unwrap();

        queue.pop().unwrap()
    }
}
