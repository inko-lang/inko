//! A list of threads managed by the VM.

use std::sync::RwLock;

use object::RcObject;

/// Struct for storing VM threads.
pub struct ThreadList {
    /// The list of threads.
    pub threads: RwLock<Vec<RcObject>>
}

impl ThreadList {
    /// Returns a new ThreadList
    pub fn new() -> ThreadList {
        ThreadList {
            threads: RwLock::new(Vec::new())
        }
    }

    /// Adds a new thread
    pub fn add(&self, thread: RcObject) {
        self.threads.write().unwrap().push(thread);
    }

    /// Removes a thread
    pub fn remove(&self, thread: RcObject) {
        let mut threads = self.threads.write().unwrap();
        let thread_id   = thread.read().unwrap().id;

        // TODO: Replace with some stdlib method
        let mut found: Option<usize> = None;

        for (index, thread) in threads.iter().enumerate() {
            if thread.read().unwrap().id == thread_id {
                found = Some(index);
            }
        }

        if found.is_some() {
            threads.remove(found.unwrap());
        }
    }

    /// Sets the prototype of all threads
    pub fn set_prototype(&self, proto: RcObject) {
        let threads = self.threads.read().unwrap();

        for thread in threads.iter() {
            thread.write().unwrap().set_prototype(proto.clone());
        }
    }

    /// Stops all threads
    pub fn stop(&self) {
        let threads = self.threads.read().unwrap();

        for thread in threads.iter() {
            let thread_obj = thread.write().unwrap();
            let vm_thread  = thread_obj.value.unwrap_thread();

            vm_thread.stop();

            let join_handle = vm_thread.take_join_handle();

            if join_handle.is_some() {
                join_handle.unwrap().join().unwrap();
            }
        }
    }
}
