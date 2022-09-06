//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.
use crate::mem::MethodPointer;
use crate::mem::Pointer;
use crate::registers::Registers;

/// A single call frame, its variables, registers, and more.
pub(crate) struct ExecutionContext {
    /// The code that we're currently running.
    pub method: MethodPointer,

    /// The parent execution context.
    pub parent: Option<Box<ExecutionContext>>,

    /// The index of the instruction to store prior to suspending a process.
    pub index: usize,

    /// The registers for this context.
    registers: Registers,
}

/// Struct for iterating over an ExecutionContext and its parent contexts.
pub(crate) struct ExecutionContextIterator<'a> {
    current: Option<&'a ExecutionContext>,
}

impl ExecutionContext {
    pub(crate) fn new(method: MethodPointer) -> Self {
        ExecutionContext {
            registers: Registers::new(method.registers),
            method,
            parent: None,
            index: 0,
        }
    }

    /// Returns the method that is being executed.
    pub(crate) fn method(&self) -> MethodPointer {
        self.method
    }

    /// Returns the value of a single register.
    pub(crate) fn get_register(&self, register: u16) -> Pointer {
        self.registers.get(register)
    }

    /// Sets the value of a single register.
    pub(crate) fn set_register(&mut self, register: u16, value: Pointer) {
        self.registers.set(register, value);
    }

    /// Returns an iterator for traversing the context chain, including the
    /// current context.
    pub(crate) fn contexts(&self) -> ExecutionContextIterator {
        ExecutionContextIterator { current: Some(self) }
    }
}

impl<'a> Iterator for ExecutionContextIterator<'a> {
    type Item = &'a ExecutionContext;

    fn next(&mut self) -> Option<&'a ExecutionContext> {
        if let Some(ctx) = self.current {
            if let Some(parent) = ctx.parent.as_ref() {
                self.current = Some(&**parent);
            } else {
                self.current = None;
            }

            return Some(ctx);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Method;
    use crate::test::empty_method;
    use std::mem;

    fn with_context<F: FnOnce(ExecutionContext)>(callback: F) {
        let method = empty_method();
        let ctx = ExecutionContext::new(method);

        callback(ctx);
        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_new() {
        let method = empty_method();
        let ctx = ExecutionContext::new(method);

        assert!(ctx.method.as_pointer() == method.as_pointer());
        assert!(ctx.parent.is_none());
        assert_eq!(ctx.index, 0);

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_for_method() {
        let method = empty_method();
        let ctx = ExecutionContext::new(method);

        assert!(ctx.method.as_pointer() == method.as_pointer());
        assert!(ctx.parent.is_none());
        assert_eq!(ctx.index, 0);

        Method::drop_and_deallocate(method);
    }

    #[test]
    fn test_get_set_register() {
        with_context(|mut ctx| {
            let ptr = Pointer::int(42);

            ctx.set_register(0, ptr);

            assert_eq!(ctx.get_register(0), ptr);
        });
    }

    #[test]
    fn test_contexts() {
        with_context(|ctx| {
            let mut iter = ctx.contexts();

            assert!(iter.next().is_some());
            assert!(iter.next().is_none());
        });
    }

    #[test]
    fn test_type_size() {
        assert_eq!(mem::size_of::<ExecutionContext>(), 40);
    }
}
