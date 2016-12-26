//! Garbage Collection Requests
//!
//! A garbage collection request specifies what to collect (a heap or mailbox),
//! and what process to collect.

use process::RcProcess;
use thread::RcThread;

pub enum CollectionType {
    Heap,
    Mailbox,
}

pub struct Request {
    pub collection_type: CollectionType,
    pub thread: RcThread,
    pub process: RcProcess,
}

impl Request {
    pub fn new(collection_type: CollectionType,
               thread: RcThread,
               process: RcProcess)
               -> Self {
        Request {
            collection_type: collection_type,
            thread: thread,
            process: process,
        }
    }

    pub fn heap(thread: RcThread, process: RcProcess) -> Self {
        Self::new(CollectionType::Heap, thread, process)
    }

    pub fn mailbox(thread: RcThread, process: RcProcess) -> Self {
        Self::new(CollectionType::Mailbox, thread, process)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compiled_code::CompiledCode;
    use immix::global_allocator::GlobalAllocator;
    use immix::permanent_allocator::PermanentAllocator;
    use process::{Process, RcProcess};
    use thread::Thread;

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
    fn test_new() {
        let (_perm, process) = new_process();
        let thread = Thread::new(None);
        let request = Request::new(CollectionType::Heap, thread, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_heap() {
        let (_perm, process) = new_process();
        let thread = Thread::new(None);
        let request = Request::heap(thread, process);

        assert!(match request.collection_type {
            CollectionType::Heap => true,
            CollectionType::Mailbox => false,
        });
    }

    #[test]
    fn test_mailbox() {
        let (_perm, process) = new_process();
        let thread = Thread::new(None);
        let request = Request::mailbox(thread, process);

        assert!(match request.collection_type {
            CollectionType::Heap => false,
            CollectionType::Mailbox => true,
        });
    }
}
