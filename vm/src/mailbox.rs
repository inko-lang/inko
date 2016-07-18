use std::collections::VecDeque;

use heap::Heap;
use object_pointer::ObjectPointer;

pub struct Mailbox {
    messages: VecDeque<ObjectPointer>,
    heap: Heap,
}

impl Mailbox {
    pub fn new() -> Mailbox {
        Mailbox {
            messages: VecDeque::new(),
            heap: Heap::local(),
        }
    }

    pub fn send(&mut self, message: ObjectPointer) {
        let mut to_send = message;

        // Instead of using is_local we can use an enum with two variants:
        // Remote and Local. A Remote message requires copying the message into
        // the message heap, a Local message can be used as-is.
        //
        // When we receive() a Remote message we copy it to the eden heap. If
        // the message is a Local message we just leave things as-is.
        //
        // This can also be used for globals as when sending a global object as
        // a message we can just use the Local variant.
        if to_send.is_local() {
            to_send = self.heap.copy_object(to_send);
        }

        self.messages.push_back(to_send);
    }

    pub fn receive(&mut self) -> Option<ObjectPointer> {
        self.messages.pop_front()
    }
}
