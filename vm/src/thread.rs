//! Virtual Machine Threads

use std::sync::{Arc, Mutex, Condvar};
use std::thread;

use process::RcProcess;

pub type RcThread = Arc<Thread>;
pub type JoinHandle = thread::JoinHandle<()>;

pub struct Thread {
    pub process_queue: Mutex<Vec<RcProcess>>,
    pub queue_added: Mutex<bool>,
    pub queue_signaler: Condvar,
    pub should_stop: Mutex<bool>,
    pub join_handle: Mutex<Option<JoinHandle>>,
    pub isolated: Mutex<bool>
}

impl Thread {
    pub fn new(handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            process_queue: Mutex::new(Vec::new()),
            queue_added: Mutex::new(false),
            queue_signaler: Condvar::new(),
            should_stop: Mutex::new(false),
            join_handle: Mutex::new(handle),
            isolated: Mutex::new(false)
        };

        Arc::new(thread)
    }

    pub fn isolated(handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread::new(handle);

        *thread.isolated.lock().unwrap() = true;

        thread
    }

    pub fn stop(&self) {
        let mut guard = self.should_stop.lock().unwrap();

        *guard = true
    }

    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        self.join_handle.lock().unwrap().take()
    }

    pub fn should_stop(&self) -> bool {
        *self.should_stop.lock().unwrap()
    }

    pub fn is_isolated(&self) -> bool {
        *self.isolated.lock().unwrap()
    }

    pub fn process_queue_size(&self) -> usize {
        self.process_queue.lock().unwrap().len()
    }

    pub fn schedule(&self, task: RcProcess) {
        let mut queue = self.process_queue.lock().unwrap();
        let mut added = self.queue_added.lock().unwrap();

        queue.push(task);
        *added = true;

        self.queue_signaler.notify_all();
    }

    pub fn wait_until_process_available(&self) {
        let empty = self.process_queue_size() == 0;

        if empty {
            let mut added = self.queue_added.lock().unwrap();

            while !*added {
                added = self.queue_signaler.wait(added).unwrap();
            }
        }
    }

    pub fn pop_process(&self) -> RcProcess {
        let mut queue = self.process_queue.lock().unwrap();
        let mut added = self.queue_added.lock().unwrap();

        *added = false;

        queue.pop().unwrap()
    }
}
