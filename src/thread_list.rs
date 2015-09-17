//! A list of threads managed by the VM.

use std::sync::RwLock;

use object::RcObject;

/// Struct for storing VM threads.
pub struct ThreadList {
    /// The list of threads.
    pub threads: RwLock<Vec<RcObject>>
}

impl ThreadList {
    pub fn new() -> ThreadList {
        ThreadList {
            threads: RwLock::new(Vec::new())
        }
    }

    pub fn add(&self, thread: RcObject) {
        write_lock!(self.threads).push(thread);
    }

    pub fn remove(&self, thread: RcObject) {
        let mut threads = write_lock!(self.threads);
        let thread_id   = read_lock!(thread).id;

        // TODO: Replace with some stdlib method
        let mut found: Option<usize> = None;

        for (index, thread) in threads.iter().enumerate() {
            if read_lock!(thread).id == thread_id {
                found = Some(index);
            }
        }

        if found.is_some() {
            threads.remove(found.unwrap());
        }
    }

    /// Sets the prototype of all threads
    pub fn set_prototype(&self, proto: RcObject) {
        let threads = read_lock!(self.threads);

        for thread in threads.iter() {
            write_lock!(thread).set_prototype(proto.clone());
        }
    }

    pub fn stop(&self) {
        let threads = read_lock!(self.threads);

        for thread in threads.iter() {
            let vm_thread = read_lock!(thread).value.as_thread();

            vm_thread.stop();

            let join_handle = vm_thread.take_join_handle();

            if join_handle.is_some() {
                join_handle.unwrap().join().unwrap();
            }
        }
    }
}
