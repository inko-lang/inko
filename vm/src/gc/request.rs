use std::collections::VecDeque;

use object_pointer::ObjectPointer;
use process::RcProcess;

pub enum Generation {
    Eden,
    Young,
    Mature,
}

pub struct Request {
    pub generation: Generation,
    pub process: RcProcess,
    pub roots: VecDeque<ObjectPointer>,
}

impl Request {
    pub fn new(generation: Generation,
               process: RcProcess,
               roots: VecDeque<ObjectPointer>)
               -> Self {
        Request {
            generation: generation,
            process: process,
            roots: roots,
        }
    }
}
