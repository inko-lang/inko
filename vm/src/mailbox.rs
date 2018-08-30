use parking_lot::Mutex;
use std::collections::LinkedList;

use config::Config;
use gc::work_list::WorkList;
use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;
use immix::mailbox_allocator::MailboxAllocator;
use object_pointer::ObjectPointer;

pub struct Mailbox {
    /// Messages sent from external processes.
    pub external: LinkedList<ObjectPointer>,

    /// Messages that were moved from the external to the internal queue.
    pub internal: LinkedList<ObjectPointer>,

    /// Messages sent by the owning process itself.
    pub locals: LinkedList<ObjectPointer>,

    /// The allocator to use for storing messages.
    pub allocator: MailboxAllocator,

    /// A lock to use when synchronising various operations, such as sending
    /// messages from external processes.
    pub write_lock: Mutex<()>,
}

impl Mailbox {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> Self {
        Mailbox {
            external: LinkedList::new(),
            internal: LinkedList::new(),
            locals: LinkedList::new(),
            allocator: MailboxAllocator::new(global_allocator, config),
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

    pub fn receive(&mut self) -> (bool, Option<ObjectPointer>) {
        if let Some(pointer) = self.locals.pop_front() {
            return (false, Some(pointer));
        }

        if self.internal.is_empty() {
            let _lock = self.write_lock.lock();

            self.internal.append(&mut self.external);
        }

        (true, self.internal.pop_front())
    }

    pub fn has_local_pointers(&self) -> bool {
        !self.locals.is_empty()
    }

    /// This method is unsafe because it does not explicitly synchronise access
    /// to `self.external`, instead this is up to the caller.
    pub unsafe fn mailbox_pointers(&self) -> WorkList {
        let mut pointers = WorkList::new();

        for pointer in self.internal.iter().chain(self.external.iter()) {
            pointers.push(pointer.pointer());
        }

        pointers
    }

    pub fn local_pointers(&self) -> WorkList {
        let mut pointers = WorkList::new();

        for pointer in &self.locals {
            pointers.push(pointer.pointer());
        }

        pointers
    }

    /// Returns true if the process has any messages available.
    ///
    /// This method should only be called when the owning processes is suspended
    /// as otherwise the counts returned could be inaccurate.
    pub fn has_messages(&self) -> bool {
        if !self.locals.is_empty() || !self.internal.is_empty() {
            return true;
        }

        let _lock = self.write_lock.lock();

        !self.external.is_empty()
    }
}
