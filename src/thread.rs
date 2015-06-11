use std::mem;
use std::sync::{Arc, RwLock};
use std::thread;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use object::RcObject;
use register::Register;
use variable_scope::VariableScope;

/// A mutable, reference counted Thread.
pub type RcThread = Arc<RwLock<Thread>>;

/// The type of JoinHandle for threads.
pub type JoinHandle = thread::JoinHandle<()>;

/// Struct representing a VM thread.
///
/// The Thread struct represents a VM thread which in turn can be mapped to an
/// actual thread, although this is technically not required. Note that these
/// are _not_ green threads, instead the VM uses regular threads and creates a
/// new Thread struct for every OS thread.
///
/// The Thread struct stores information such as the current call frame and the
/// native Rust thread bound to this structure.
///
pub struct Thread {
    /// The current call frame.
    pub call_frame: CallFrame,

    /// The return value of the thread, if any.
    pub value: Option<RcObject>,

    /// Boolean indicating if this is the main thread.
    pub main_thread: bool,

    /// The JoinHandle of the current thread.
    join_handle: Option<JoinHandle>,
}

impl Thread {
    /// Creates a new Thread.
    pub fn new(call_frame: CallFrame, handle: Option<JoinHandle>) -> RcThread {
        let thread = Thread {
            call_frame: call_frame,
            value: None,
            main_thread: false,
            join_handle: handle
        };

        Arc::new(RwLock::new(thread))
    }

    /// Creates a new Thread from a CompiledCode/CallFrame.
    pub fn from_code(code: RcCompiledCode,
                     handle: Option<JoinHandle>) -> RcThread {
        let frame = CallFrame::from_code(code);

        Thread::new(frame, handle)
    }

    /// Sets the current CallFrame from a CompiledCode.
    pub fn push_call_frame(&mut self, mut frame: CallFrame) {
        mem::swap(&mut self.call_frame, &mut frame);

        self.call_frame.set_parent(frame);
    }

    /// Switches the current call frame to the previous one.
    pub fn pop_call_frame(&mut self) {
        let parent = self.call_frame.parent.take().unwrap();

        // TODO: this might move the data from heap back to the stack?
        self.call_frame = *parent;
    }

    /// Returns a reference to the current call frame.
    pub fn call_frame(&self) -> &CallFrame {
        &self.call_frame
    }

    /// Returns a mutable reference to the current register.
    pub fn register(&mut self) -> &mut Register {
        &mut self.call_frame.register
    }

    /// Returns a mutable reference to the current variable scope.
    pub fn variable_scope(&mut self) -> &mut VariableScope {
        &mut self.call_frame.variables
    }

    /// Marks the current thread as the main thread.
    pub fn set_main(&mut self) {
        self.main_thread = true;
    }

    /// Consumes and returns the JoinHandle.
    pub fn take_join_handle(&mut self) -> Option<JoinHandle> {
        self.join_handle.take()
    }
}
