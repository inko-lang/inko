//! Helper functions for writing unit tests.
use crate::config::Config;
use crate::location_table::LocationTable;
use crate::mem::{
    Class, ClassPointer, Method, MethodPointer, Module, ModulePointer,
};
use crate::permanent_space::{MethodCounts, PermanentSpace};
use crate::process::{Process, ProcessPointer};
use crate::scheduler::process::Thread;
use crate::state::{RcState, State};
use bytecode::{Instruction, Opcode};
use std::mem::forget;
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
pub(crate) struct OwnedClass(ClassPointer);

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
        Class::drop(self.0);
    }
}

/// A module that is dropped when this pointer is dropped.
#[repr(transparent)]
pub(crate) struct OwnedModule(ModulePointer);

impl OwnedModule {
    pub(crate) fn new(ptr: ModulePointer) -> Self {
        Self(ptr)
    }
}

impl Deref for OwnedModule {
    type Target = ModulePointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedModule {
    fn drop(&mut self) {
        Module::drop_and_deallocate(self.0);
    }
}

/// Sets up various objects commonly needed in tests
pub(crate) fn setup() -> (RcState, Vec<Thread>) {
    let mut config = Config::new();

    // We set this to a fixed amount so tests are more consistent across
    // different platforms.
    config.process_threads = 2;

    let perm = PermanentSpace::new(0, 0, MethodCounts::default());

    State::new(config, perm, &[])
}

pub(crate) fn new_process(class: ClassPointer) -> OwnedProcess {
    OwnedProcess::new(Process::alloc(class))
}

pub(crate) fn new_main_process(
    class: ClassPointer,
    method: MethodPointer,
) -> OwnedProcess {
    OwnedProcess::new(Process::main(class, method))
}

pub(crate) fn empty_module(class: ClassPointer) -> OwnedModule {
    OwnedModule::new(Module::alloc(class))
}

pub(crate) fn empty_method() -> MethodPointer {
    // We use Int values for the name so we don't have to worry about also
    // dropping strings when dropping this Method.
    Method::alloc(
        123,
        2,
        vec![Instruction::new(Opcode::Return, [0; 5])],
        LocationTable::new(),
        Vec::new(),
    )
}

pub(crate) fn empty_async_method() -> MethodPointer {
    // We use Int values for the name so we don't have to worry about also
    // dropping strings when dropping this Method.
    Method::alloc(
        123,
        2,
        vec![Instruction::one(Opcode::ProcessFinishTask, 1)],
        LocationTable::new(),
        Vec::new(),
    )
}

pub(crate) fn empty_class(name: &str) -> OwnedClass {
    OwnedClass::new(Class::alloc(name.to_string(), 0, 0))
}

pub(crate) fn empty_process_class(name: &str) -> OwnedClass {
    OwnedClass::new(Class::process(name.to_string(), 0, 0))
}
