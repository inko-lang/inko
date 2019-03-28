use crate::config::Config;
use crate::gc::work_list::WorkList;
use crate::immix::copy_object::CopyObject;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::immix::mailbox_allocator::MailboxAllocator;
use crate::object_pointer::ObjectPointer;
use parking_lot::Mutex;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Mailbox {
    /// Messages sent from external processes.
    pub external: LinkedList<ObjectPointer>,

    /// Messages that were moved from the external to the internal queue, or
    /// were sent by the owning process itself.
    pub internal: LinkedList<ObjectPointer>,

    /// The allocator to use for storing messages.
    pub allocator: MailboxAllocator,

    /// A lock to use when synchronising various operations, such as sending
    /// messages from external processes.
    pub write_lock: Mutex<()>,

    /// The number of messages stored in this mailbox.
    ///
    /// Since messages can be stored in either the synchronised external half
    /// or the unsynchronised internal half, obtaining this number would be
    /// expensive. Storing it separately and using atomic operations to access
    /// it allows us to more efficiently retrieve this number, at the cost of a
    /// little bit of extra memory.
    amount: AtomicUsize,
}

impl Mailbox {
    pub fn new(global_allocator: RcGlobalAllocator, config: &Config) -> Self {
        Mailbox {
            external: LinkedList::new(),
            internal: LinkedList::new(),
            allocator: MailboxAllocator::new(global_allocator, config),
            write_lock: Mutex::new(()),
            amount: AtomicUsize::new(0),
        }
    }

    pub fn send_from_external(&mut self, original: ObjectPointer) {
        let _lock = self.write_lock.lock();

        self.external
            .push_back(self.allocator.copy_object(original));

        self.amount.fetch_add(1, Ordering::AcqRel);
    }

    pub fn send_from_self(&mut self, pointer: ObjectPointer) {
        self.internal.push_back(pointer);
        self.amount.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns a tuple containing a boolean and an optional message.
    ///
    /// If the boolean is set to `true`, the returned pointer must be copied to
    /// a process' local heap.
    pub fn receive(&mut self) -> (bool, Option<ObjectPointer>) {
        if self.internal.is_empty() {
            let _lock = self.write_lock.lock();

            self.internal.append(&mut self.external);
        }

        if let Some(pointer) = self.internal.pop_front() {
            self.amount.fetch_sub(1, Ordering::AcqRel);

            return (pointer.is_mailbox(), Some(pointer));
        } else {
            (false, None)
        }
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

        for pointer in &self.internal {
            if !pointer.is_mailbox() {
                pointers.push(pointer.pointer());
            }
        }

        pointers
    }

    pub fn has_messages(&self) -> bool {
        self.amount.load(Ordering::Acquire) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::object_pointer::ObjectPointer;
    use crate::vm::test::setup;

    #[test]
    fn test_send_from_self() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        assert_eq!(mailbox.amount.load(Ordering::Acquire), 0);

        mailbox.send_from_self(ObjectPointer::integer(5));

        assert_eq!(mailbox.amount.load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_send_from_external() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        assert_eq!(mailbox.amount.load(Ordering::Acquire), 0);

        mailbox.send_from_external(ObjectPointer::integer(5));

        assert_eq!(mailbox.amount.load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_receive_without_messages() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        let (must_copy, message) = mailbox.receive();

        assert_eq!(must_copy, false);
        assert!(message.is_none());
    }

    #[test]
    fn test_receive_with_external_message() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        mailbox.send_from_external(ObjectPointer::integer(5));

        let (must_copy, message) = mailbox.receive();

        assert_eq!(must_copy, false);
        assert_eq!(mailbox.amount.load(Ordering::Acquire), 0);
        assert!(message == Some(ObjectPointer::integer(5)));
    }

    #[test]
    fn test_receive_with_external_message_with_copying() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);
        let (_machine, _block, process) = setup();

        let message = process.allocate_empty();

        mailbox.send_from_external(message);

        let (must_copy, message) = mailbox.receive();

        assert_eq!(must_copy, true);
        assert_eq!(mailbox.amount.load(Ordering::Acquire), 0);
        assert!(message.unwrap().get().value.is_none());
    }

    #[test]
    fn test_receive_with_internal_message() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        mailbox.send_from_self(ObjectPointer::integer(5));

        let (must_copy, message) = mailbox.receive();

        assert_eq!(must_copy, false);
        assert_eq!(mailbox.amount.load(Ordering::Acquire), 0);
        assert!(message == Some(ObjectPointer::integer(5)));
    }

    #[test]
    fn test_has_messages() {
        let config = Config::new();
        let mut mailbox = Mailbox::new(GlobalAllocator::with_rc(), &config);

        assert_eq!(mailbox.has_messages(), false);

        mailbox.send_from_self(ObjectPointer::integer(5));

        assert!(mailbox.has_messages());
    }
}
