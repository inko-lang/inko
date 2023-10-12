//! Helper functions for writing unit tests.
use crate::config::Config;
use crate::mem::{Class, ClassPointer};
use crate::process::{NativeAsyncMethod, Process, ProcessPointer};
use crate::stack::Stack;
use crate::state::{MethodCounts, RcState, State};
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

/// A class that is dropped when this pointer is dropped.
#[repr(transparent)]
pub(crate) struct OwnedClass(pub(crate) ClassPointer);

impl OwnedClass {
    pub(crate) fn new(ptr: ClassPointer) -> Self {
        Self(ptr)
    }
}

impl Deref for OwnedClass {
    type Target = ClassPointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedClass {
    fn drop(&mut self) {
        unsafe {
            Class::drop(self.0);
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

pub(crate) fn new_process(class: ClassPointer) -> OwnedProcess {
    OwnedProcess::new(Process::alloc(class, Stack::new(1024)))
}

pub(crate) fn new_main_process(
    class: ClassPointer,
    method: NativeAsyncMethod,
) -> OwnedProcess {
    OwnedProcess::new(Process::main(class, method, Stack::new(1024)))
}

pub(crate) fn empty_process_class(name: &str) -> OwnedClass {
    OwnedClass::new(Class::process(
        name.to_string(),
        size_of::<Process>() as _,
        0,
    ))
}
