use process::RcProcess;
use thread::RcThread;

pub enum Generation {
    Young,
    Mailbox,
}

impl Generation {
    pub fn is_young(&self) -> bool {
        match self {
            &Generation::Young => true,
            _ => false,
        }
    }
}

pub struct Request {
    pub generation: Generation,
    pub thread: RcThread,
    pub process: RcProcess,
}

impl Request {
    pub fn new(generation: Generation,
               thread: RcThread,
               process: RcProcess)
               -> Self {
        Request {
            generation: generation,
            thread: thread,
            process: process,
        }
    }
}
