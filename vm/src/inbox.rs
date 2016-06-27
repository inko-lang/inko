use std::collections::VecDeque;

use heap::Heap;
use object_pointer::ObjectPointer;

pub struct Inbox {
    messages: VecDeque<ObjectPointer>,
    heap: Heap,
}

impl Inbox {
    pub fn new() -> Inbox {
        Inbox {
            messages: VecDeque::new(),
            heap: Heap::local(),
        }
    }

    pub fn send(&mut self, message: ObjectPointer) {
        let mut to_send = message;

        if to_send.is_local() {
            to_send = self.heap.copy_object(to_send);
        }

        self.messages.push_back(to_send);
    }

    pub fn receive(&mut self) -> Option<ObjectPointer> {
        self.messages.pop_front()
    }
}
