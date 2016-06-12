use std::sync::{Arc, Condvar, Mutex};
use object_pointer::ObjectPointer;

pub struct Inbox {
    messages: Mutex<Vec<ObjectPointer>>,
    received: Mutex<bool>,
    signaler: Condvar,
}

pub type RcInbox = Arc<Inbox>;

impl Inbox {
    pub fn new() -> RcInbox {
        let inbox = Inbox {
            messages: Mutex::new(Vec::new()),
            received: Mutex::new(false),
            signaler: Condvar::new(),
        };

        Arc::new(inbox)
    }

    pub fn send(&self, message: ObjectPointer) {
        let mut messages = self.messages.lock().unwrap();
        let mut received = self.received.lock().unwrap();

        messages.push(message);
        *received = true;

        self.signaler.notify_all();
    }

    pub fn empty(&self) -> bool {
        self.messages.lock().unwrap().len() == 0
    }

    pub fn receive(&self) -> ObjectPointer {
        if self.empty() {
            let mut received = self.received.lock().unwrap();

            while !*received {
                received = self.signaler.wait(received).unwrap();
            }
        }

        self.messages.lock().unwrap().pop().unwrap()
    }
}
