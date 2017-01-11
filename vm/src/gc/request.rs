//! Garbage Collection Requests
//!
//! A garbage collection request specifies what to collect (a heap or mailbox),
//! and what process to collect.

use gc::heap_collector;
use gc::mailbox_collector;
use process::RcProcess;
use vm::state::RcState;

pub enum CollectionType {
    Heap,
    Mailbox,
}

pub struct Request {
    pub vm_state: RcState,
    pub collection_type: CollectionType,
    pub process: RcProcess,
}

impl Request {
    pub fn new(collection_type: CollectionType,
               vm_state: RcState,
               process: RcProcess)
               -> Self {
        Request {
            vm_state: vm_state,
            collection_type: collection_type,
            process: process,
        }
    }

    /// Returns a request for collecting a process' heap.
    pub fn heap(vm_state: RcState, process: RcProcess) -> Self {
        Self::new(CollectionType::Heap, vm_state, process)
    }

    /// Returns a request for collecting a process' mailbox.
    pub fn mailbox(vm_state: RcState, process: RcProcess) -> Self {
        Self::new(CollectionType::Mailbox, vm_state, process)
    }

    /// Performs the garbage collection request.
    pub fn perform(&self) {
        // If we know the process has already been terminated there's no
        // point in performing a collection.
        if !self.process.is_alive() {
            return;
        }

        // TODO: store profile details
        let _profile = match self.collection_type {
            CollectionType::Heap => {
                heap_collector::collect(&self.vm_state, &self.process)
            }
            CollectionType::Mailbox => {
                mailbox_collector::collect(&self.vm_state, &self.process)
            }
        };

        println!("Finished {:?} collection in {:.2} ms",
                 _profile.collection_type,
                 _profile.total.duration_msec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use config::Config;
    use immix::global_allocator::GlobalAllocator;
    use immix::permanent_allocator::PermanentAllocator;
    use process::{Process, RcProcess};
    use vm::state::State;

    fn new_process() -> (Box<PermanentAllocator>, RcProcess) {
        let global_alloc = GlobalAllocator::without_preallocated_blocks();

        let mut perm_alloc =
            Box::new(PermanentAllocator::new(global_alloc.clone()));

        let self_obj = perm_alloc.allocate_empty();

        let code = CompiledCode::with_rc("a".to_string(),
                                         "a".to_string(),
                                         1,
                                         Vec::new());

        (perm_alloc, Process::from_code(1, 0, code, self_obj, global_alloc))
    }

    #[test]
    fn test_new() {
        let (_perm, process) = new_process();
        let state = State::new(Config::new());
        let request = Request::new(CollectionType::Heap, state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_heap() {
        let (_perm, process) = new_process();
        let state = State::new(Config::new());
        let request = Request::heap(state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_mailbox() {
        let (_perm, process) = new_process();
        let state = State::new(Config::new());
        let request = Request::mailbox(state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => false,
            CollectionType::Mailbox => true,
        });
    }

    #[test]
    fn test_perform() {
        let (_perm, process) = new_process();
        let state = State::new(Config::new());
        let request = Request::heap(state, process.clone());

        process.set_register(0, process.allocate_empty());
        process.running();
        request.perform();

        assert!(process.get_register(0).unwrap().is_marked());
    }
}
