//! A list of threads managed by the VM.

use object::RcObject;

/// Struct for storing VM threads.
pub struct ThreadList {
    pub threads: Vec<RcObject>
}

impl ThreadList {
    pub fn new() -> ThreadList {
        ThreadList {
            threads: Vec::new()
        }
    }

    pub fn add(&mut self, thread: RcObject) {
        self.threads.push(thread);
    }

    pub fn remove(&mut self, thread: RcObject) {
        let thread_id = read_lock!(thread).id;

        // TODO: Replace with some stdlib method
        let mut found: Option<usize> = None;

        for (index, thread) in self.threads.iter().enumerate() {
            if read_lock!(thread).id == thread_id {
                found = Some(index);
            }
        }

        if found.is_some() {
            self.threads.remove(found.unwrap());
        }
    }

    /// Sets the prototype of all threads
    pub fn set_prototype(&mut self, proto: RcObject) {
        for thread in self.threads.iter() {
            write_lock!(thread).set_prototype(proto.clone());
        }
    }

    pub fn stop(&mut self) {
        for thread in self.threads.iter() {
            let vm_thread = read_lock!(thread).value.as_thread();

            vm_thread.stop();

            let join_handle = vm_thread.take_join_handle();

            if join_handle.is_some() {
                join_handle.unwrap().join().unwrap();
            }
        }
    }
}
