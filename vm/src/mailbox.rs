use parking_lot::Mutex;
use std::collections::VecDeque;

use config::Config;
use gc::work_list::WorkList;
use immix::copy_object::CopyObject;
use immix::global_allocator::RcGlobalAllocator;
use immix::mailbox_allocator::MailboxAllocator;
use object_pointer::ObjectPointer;

pub struct Mailbox {
    pub external: VecDeque<ObjectPointer>,
    pub internal: VecDeque<ObjectPointer>,
    pub locals: VecDeque<ObjectPointer>,
    pub allocator: MailboxAllocator,
    pub write_lock: Mutex<()>,
}

impl Mailbox {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> Self {
        Mailbox {
            external: VecDeque::new(),
            internal: VecDeque::new(),
            locals: VecDeque::new(),
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

            self.internal
                .append(&mut self.external.drain(0..).collect());
        }

        (true, self.internal.pop_front())
    }

    pub fn has_local_pointers(&self) -> bool {
        !self.locals.is_empty()
    }

    pub fn mailbox_pointers(&self) -> WorkList {
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
