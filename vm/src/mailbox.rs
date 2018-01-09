use std::collections::VecDeque;
use parking_lot::Mutex;

use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;
use immix::mailbox_allocator::MailboxAllocator;
use object_pointer::{ObjectPointer, ObjectPointerPointer};

pub struct Mailbox {
    pub external: VecDeque<ObjectPointer>,
    pub internal: VecDeque<ObjectPointer>,
    pub locals: VecDeque<ObjectPointer>,
    pub allocator: MailboxAllocator,
    pub write_lock: Mutex<()>,
}

impl Mailbox {
    pub fn new(global_allocator: RcGlobalAllocator) -> Self {
        Mailbox {
            external: VecDeque::new(),
            internal: VecDeque::new(),
            locals: VecDeque::new(),
            allocator: MailboxAllocator::new(global_allocator),
            write_lock: Mutex::new(()),
        }
    }

    pub fn send_from_external(&mut self, original: ObjectPointer) {
        let _lock = self.write_lock.lock();

        self.external
            .push_back(self.allocator.copy_object(original));
    }

    pub fn send_from_self(&mut self, pointer: ObjectPointer) {
        self.locals.push_back(pointer);
    }

    pub fn receive(&mut self) -> Option<ObjectPointer> {
        if let Some(pointer) = self.locals.pop_front() {
            return Some(pointer);
        }

        if self.internal.len() == 0 {
            let _lock = self.write_lock.lock();

            self.internal
                .append(&mut self.external.drain(0..).collect());
        }

        self.internal.pop_front()
    }

    pub fn has_local_pointers(&self) -> bool {
        self.locals.len() > 0
    }

    pub fn mailbox_pointers(&self) -> Vec<ObjectPointerPointer> {
        self.internal
            .iter()
            .chain(self.external.iter())
            .map(|p| p.pointer())
            .collect()
    }

    pub fn local_pointers(&self) -> Vec<ObjectPointerPointer> {
        self.locals
            .iter()
            .map(|pointer| pointer.pointer())
            .collect()
    }

    /// Returns true if the process has any messages available.
    ///
    /// This method should only be called when the owning processes is suspended
    /// as otherwise the counts returned could be inaccurate.
    pub fn has_messages(&self) -> bool {
        if self.locals.len() > 0 || self.internal.len() > 0 {
            return true;
        }

        let _lock = self.write_lock.lock();

        self.external.len() > 0
    }
}
