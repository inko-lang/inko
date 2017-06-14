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
        // TODO: store profile details
        let _profile = match self.collection_type {
            CollectionType::Heap => {
                heap_collector::collect(&self.vm_state, &self.process)
            }
            CollectionType::Mailbox => {
                mailbox_collector::collect(&self.vm_state, &self.process)
            }
        };

        println!(
            "Finished {:?} collection in {:.2} ms, {} marked, {} \
                  promoted, {} evacuated",
            _profile.collection_type,
            _profile.total.duration_msec(),
            _profile.marked,
            _profile.promoted,
            _profile.evacuated
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Config;
    use vm::state::State;
    use vm::test::setup;

    #[test]
    fn test_new() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new());
        let request = Request::new(CollectionType::Heap, state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_heap() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new());
        let request = Request::heap(state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_mailbox() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new());
        let request = Request::mailbox(state, process);

        assert!(match request.collection_type {
            CollectionType::Heap => false,
            CollectionType::Mailbox => true,
        });
    }

    #[test]
    fn test_perform() {
        let (_machine, _block, process) = setup();
        let state = State::new(Config::new());
        let request = Request::heap(state, process.clone());

        process.set_register(0, process.allocate_empty());
        process.running();
        request.perform();

        assert!(process.get_register(0).is_marked());
    }
}
