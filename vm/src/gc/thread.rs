//! Threads for garbage collecting memory.

use gc::heap_collector;
use gc::mailbox_collector;
use gc::request::{Request, CollectionType};
use vm::state::RcState;

/// Structure containing the state of a single GC thread.
pub struct Thread {
    pub vm_state: RcState,
}

impl Thread {
    pub fn new(vm_state: RcState) -> Thread {
        Thread { vm_state: vm_state }
    }

    pub fn run(&mut self) {
        loop {
            let request = self.vm_state.gc_requests.pop();

            self.process_request(request);
        }
    }

    fn process_request(&self, request: Request) {
        // If we know the process has already been terminated there's no
        // point in performing a collection.
        if !request.process.is_alive() {
            return;
        }

        let ref config = self.vm_state.config;

        // TODO: store profile details
        let _profile = match request.collection_type {
            CollectionType::Heap => {
                heap_collector::collect(&request.thread,
                                        &request.process,
                                        &config)
            }
            CollectionType::Mailbox => {
                mailbox_collector::collect(&request.thread,
                                           &request.process,
                                           &config)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use config::Config;
    use gc::request::Request;
    use immix::global_allocator::GlobalAllocator;
    use immix::permanent_allocator::PermanentAllocator;
    use process::{Process, RcProcess};
    use thread::Thread as VmThread;
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

        (perm_alloc, Process::from_code(1, code, self_obj, global_alloc))
    }

    #[test]
    fn test_process_request() {
        let (_perm, process) = new_process();

        process.set_register(0, process.allocate_empty());
        process.running();

        let vm_thread = VmThread::new(None);
        let vm_state = State::new(Config::new());
        let gc_thread = Thread::new(vm_state);

        process.running();

        gc_thread.process_request(Request::heap(vm_thread, process.clone()));

        assert!(process.get_register(0).unwrap().is_marked());
    }
}
