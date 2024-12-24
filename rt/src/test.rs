//! Helper functions for writing unit tests.
use crate::config::Config;
use crate::mem::{Type, TypePointer};
use crate::process::{Message, NativeAsyncMethod, Process, ProcessPointer};
use crate::stack::Stack;
use crate::state::{MethodCounts, RcState, State};
use rustix::param::page_size;
use std::mem::{forget, size_of};
use std::ops::{Deref, DerefMut, Drop};

/// Processes normally drop themselves when they finish running. But in tests we
/// don't actually run a process.
///
/// To remove the need for manually adding `Process::drop(...)` in every test,
/// we use this wrapper type to automatically drop processes.
pub(crate) struct OwnedProcess(ProcessPointer);

impl OwnedProcess {
    pub(crate) fn new(process: ProcessPointer) -> Self {
        Self(process)
    }

    /// Returns the underlying process, and doesn't run the descructor for this
    /// wrapper.
    pub(crate) fn take_and_forget(self) -> ProcessPointer {
        let ptr = self.0;

        forget(self);
        ptr
    }
}

impl Deref for OwnedProcess {
    type Target = ProcessPointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnedProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        Process::drop_and_deallocate(self.0);
    }
}

/// A type that is dropped when this pointer is dropped.
#[repr(transparent)]
pub(crate) struct OwnedType(pub(crate) TypePointer);

impl OwnedType {
    pub(crate) fn new(ptr: TypePointer) -> Self {
        Self(ptr)
    }
}

impl Deref for OwnedType {
    type Target = TypePointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedType {
    fn drop(&mut self) {
        unsafe {
            Type::drop(self.0);
        }
    }
}

/// Sets up various objects commonly needed in tests
pub(crate) fn setup() -> RcState {
    let mut config = Config::new();

    // We set this to a fixed amount so tests are more consistent across
    // different platforms.
    config.process_threads = 2;

    State::new(config, &MethodCounts::default(), Vec::new())
}

pub(crate) fn new_process(instance_of: TypePointer) -> OwnedProcess {
    OwnedProcess::new(Process::alloc(
        instance_of,
        Stack::new(1024, page_size()),
    ))
}

pub(crate) fn new_process_with_message(
    instance_of: TypePointer,
    method: NativeAsyncMethod,
) -> OwnedProcess {
    let stack = Stack::new(1024, page_size());
    let mut proc = Process::alloc(instance_of, stack);

    // We use a custom message that takes the process as an argument. This way
    // the tests have access to the current process, without needing to fiddle
    // with the stack like the generated code does.
    let message = Message { method, data: proc.as_ptr() as _ };

    proc.send_message(message);
    OwnedProcess::new(proc)
}

pub(crate) fn empty_process_type(name: &str) -> OwnedType {
    OwnedType::new(Type::process(
        name.to_string(),
        size_of::<Process>() as _,
        0,
    ))
}
