use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use std::collections::VecDeque;

pub struct Mailbox {
    /// The messages stored in this mailbox.
    messages: VecDeque<ObjectPointer>,
}

impl Mailbox {
    pub fn new() -> Self {
        Mailbox {
            messages: VecDeque::new(),
        }
    }

    pub fn send(&mut self, message: ObjectPointer) {
        self.messages.push_back(message);
    }

    pub fn receive(&mut self) -> Option<ObjectPointer> {
        self.messages.pop_front()
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        for message in &self.messages {
            callback(message.pointer());
        }
    }

    pub fn has_messages(&self) -> bool {
        !self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_pointer::ObjectPointer;

    #[test]
    fn test_send_receive() {
        let mut mailbox = Mailbox::new();

        mailbox.send(ObjectPointer::integer(4));

        assert!(mailbox.receive() == Some(ObjectPointer::integer(4)))
    }

    #[test]
    fn test_each_pointer() {
        let mut mailbox = Mailbox::new();

        mailbox.send(ObjectPointer::new(0x1 as _));

        let mut pointers = Vec::new();

        mailbox.each_pointer(|ptr| pointers.push(ptr));

        let pointer_pointer = pointers.pop();

        assert!(pointer_pointer.is_some());

        pointer_pointer.unwrap().get_mut().raw.raw = 0x4 as _;

        assert!(mailbox.receive() == Some(ObjectPointer::new(0x4 as _)));
    }

    #[test]
    fn test_has_messagess() {
        let mut mailbox = Mailbox::new();

        assert_eq!(mailbox.has_messages(), false);

        mailbox.send(ObjectPointer::integer(5));

        assert!(mailbox.has_messages());
    }
}
