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
    /// The current call frame.
    pub call_frame: RwLock<CallFrame>,

    /// The return value of the thread, if any.
    pub value: RwLock<Option<RcObject>>,

    /// Boolean indicating if this is the main thread.
    pub main_thread: RwLock<bool>,

    /// Boolean that indicates if the current thread should be terminated.
    pub should_stop: RwLock<bool>,

    /// The JoinHandle of the current thread.
    join_handle: RwLock<Option<JoinHandle>>,
}

impl Thread {
    /// Creates a new Thread.
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

    /// Creates a new Thread from a CompiledCode/CallFrame.
    pub fn from_code(code: RcCompiledCode,
                     handle: Option<JoinHandle>) -> RcThread {
        let frame = CallFrame::from_code(code);

        Thread::new(frame, handle)
    }

    /// Sets the current CallFrame from a CompiledCode.
    pub fn push_call_frame(&self, mut frame: CallFrame) {
        let mut target = self.call_frame.write().unwrap();

        mem::swap(&mut *target, &mut frame);

        target.set_parent(frame);
    }

    /// Switches the current call frame to the previous one.
    pub fn pop_call_frame(&self) {
        let mut target = self.call_frame.write().unwrap();
        let parent     = target.parent.take().unwrap();

        // TODO: this might move the data from heap back to the stack?
        *target = *parent;
    }

    /// Marks the current thread as the main thread.
    pub fn set_main(&self) {
        *self.main_thread.write().unwrap() = true;
    }

    /// Sets the return value of the thread.
    pub fn set_value(&self, value: Option<RcObject>) {
        *self.value.write().unwrap() = value;
    }

    /// Instructs the thread to gracefully terminate itself.
    pub fn stop(&self) {
        *self.should_stop.write().unwrap() = true;
    }

    /// Consumes and returns the JoinHandle.
    pub fn take_join_handle(&self) -> Option<JoinHandle> {
        self.join_handle.write().unwrap().take()
    }

    /// Returns true if a thread should stop.
    pub fn should_stop(&self) -> bool {
        *self.should_stop.read().unwrap()
    }

    /// Gets a slot value from the current register.
    pub fn get_register(&self, slot: usize) -> Option<RcObject> {
        let frame = self.call_frame.read().unwrap();

        frame.register.get(slot)
    }

    /// Sets a slot in the current register.
    pub fn set_register(&self, slot: usize, value: RcObject) {
        let mut frame = self.call_frame.write().unwrap();

        frame.register.set(slot, value);
    }

    /// Inserts a local variable at a specific position.
    pub fn set_local(&self, index: usize, value: RcObject) {
        let mut frame = self.call_frame.write().unwrap();

        frame.variables.insert(index, value);
    }

    /// Appends a new local variable to the current scope.
    pub fn add_local(&self, value: RcObject) {
        let mut frame = self.call_frame.write().unwrap();

        frame.variables.add(value);
    }

    /// Returns a local variable from the current variable scope.
    pub fn get_local(&self, index: usize) -> Option<RcObject> {
        let frame = self.call_frame.read().unwrap();

        frame.variables.get(index)
    }
}
