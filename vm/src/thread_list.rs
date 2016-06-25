//! A list of threads managed by the VM.

use thread::{RcThread, Thread};
use thread::JoinHandle;
use process::RcProcess;

/// Struct for storing VM threads.
pub struct ThreadList {
    pub threads: Vec<RcThread>,
}

impl ThreadList {
    pub fn new() -> ThreadList {
        ThreadList { threads: Vec::new() }
    }

    pub fn add(&mut self, handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread::new(false, handle);

        self.threads.push(thread.clone());

        thread
    }

    pub fn add_main_thread(&mut self) -> RcThread {
        let thread = Thread::new(true, None);

        self.threads.push(thread.clone());

        thread
    }

    pub fn remove(&mut self, thread: RcThread) {
        let search_ptr = &*thread as *const _;

        let mut found: Option<usize> = None;

        // Threads are wrapped in an Arc so we can just compare raw pointers
        // instead of implementing Eq & friends.
        for (index, thread_lock) in self.threads.iter().enumerate() {
            let current_ptr = &**thread_lock as *const _;

            if current_ptr == search_ptr {
                found = Some(index);
                break;
            }
        }

        if found.is_some() {
            self.threads.remove(found.unwrap());
        }
    }

    pub fn stop(&mut self) {
        for thread in self.threads.iter() {
            thread.stop();

            let join_handle = thread.take_join_handle();

            if join_handle.is_some() {
                join_handle.unwrap().join().unwrap();
            }
        }
    }

    pub fn schedule(&mut self, process: RcProcess) {
        let mut thread_idx = 0;
        let mut queue_size = None;

        // Schedule the process in the thread with the least amount of processes
        // queued up.
        for (index, thread) in self.threads.iter().enumerate() {
            if queue_size.is_some() {
                let current_size = thread.process_queue_size();

                if current_size < queue_size.unwrap() {
                    thread_idx = index;
                    queue_size = Some(current_size);
                }
            } else {
                thread_idx = index;
                queue_size = Some(thread.process_queue_size());
            }
        }

        self.threads[thread_idx].schedule(process);
    }
}
