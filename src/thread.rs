//! Virtual Machine Threads
//!
//! This module can be used to create the required structures used to map a Rust
//! thread with a virtual machine thread (and thus a thread in the language
//! itself).

use std::mem;
use std::sync::{Arc, RwLock};
use std::thread;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use object::RcObject;

/// A mutable, reference counted Thread.
pub type RcThread = Arc<Thread>;

/// The type of JoinHandle for threads.
pub type JoinHandle = thread::JoinHandle<()>;

/// Struct representing a VM thread.
pub struct Thread {
    pub call_frame: RwLock<CallFrame>,

    /// The return value of the thread, if any.
    pub value: RwLock<Option<RcObject>>,
    pub main_thread: RwLock<bool>,
    pub should_stop: RwLock<bool>,

    join_handle: RwLock<Option<JoinHandle>>,
}

impl Thread {
    pub fn new(call_frame: CallFrame, handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            call_frame: RwLock::new(call_frame),
            value: RwLock::new(None),
            main_thread: RwLock::new(false),
            should_stop: RwLock::new(false),
            join_handle: RwLock::new(handle)
        };

        Arc::new(thread)
    }

    pub fn from_code(code: RcCompiledCode,
                     handle: Option<JoinHandle>) -> RcThread {
        let frame = CallFrame::from_code(code);

        Thread::new(frame, handle)
    }

    pub fn push_call_frame(&self, mut frame: CallFrame) {
        let mut target = write_lock!(self.call_frame);

        mem::swap(&mut *target, &mut frame);

        target.set_parent(frame);
    }

    pub fn pop_call_frame(&self) {
        let mut target = write_lock!(self.call_frame);
        let parent     = target.parent.take().unwrap();

        // TODO: this might move the data from heap back to the stack?
        *target = *parent;
    }

    pub fn set_main(&self) {
        *write_lock!(self.main_thread) = true;
    }

    pub fn set_value(&self, value: Option<RcObject>) {
        *write_lock!(self.value) = value;
    }

    pub fn stop(&self) {
        *write_lock!(self.should_stop) = true;
    }

    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        write_lock!(self.join_handle).take()
    }

    pub fn should_stop(&self) -> bool {
        *read_lock!(self.should_stop)
    }

    pub fn get_register(&self, slot: usize) -> Option<RcObject> {
        let frame = read_lock!(self.call_frame);

        frame.register.get(slot)
    }

    pub fn set_register(&self, slot: usize, value: RcObject) {
        let mut frame = write_lock!(self.call_frame);

        frame.register.set(slot, value);
    }

    pub fn set_local(&self, index: usize, value: RcObject) {
        let mut frame = write_lock!(self.call_frame);

        frame.variables.insert(index, value);
    }

    pub fn add_local(&self, value: RcObject) {
        let mut frame = write_lock!(self.call_frame);

        frame.variables.add(value);
    }

    pub fn get_local(&self, index: usize) -> Option<RcObject> {
        let frame = read_lock!(self.call_frame);

        frame.variables.get(index)
    }
}
