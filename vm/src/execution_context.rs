//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.

use binding::{Binding, RcBinding};
use compiled_code::RcCompiledCode;
use object_pointer::{ObjectPointer, ObjectPointerPointer};
use register::Register;

pub struct ExecutionContext {
    /// The registers for this context.
    pub register: Register,

    /// The binding to evaluate this context in.
    pub binding: RcBinding,

    /// The CompiledCodea object associated with this context.
    pub code: RcCompiledCode,

    /// The parent execution context.
    pub parent: Option<Box<ExecutionContext>>,

    /// The index of the instruction to store prior to suspending a process.
    pub instruction_index: usize,

    /// The register to store this context's return value in.
    pub return_register: Option<usize>,

    /// The current line that is being executed.
    pub line: u16,
}

// While an ExecutionContext is not thread-safe we need to implement Sync/Send
// so we can process a call stack in parallel. Since a stack can not be modified
// during a collection we can safely share contexts between threads during this
// time.
unsafe impl Sync for ExecutionContext {}
unsafe impl Send for ExecutionContext {}

/// Struct for iterating over an ExecutionContext and its parent contexts.
pub struct ExecutionContextIterator<'a> {
    current: Option<&'a ExecutionContext>,
}

impl ExecutionContext {
    pub fn new(binding: RcBinding,
               code: RcCompiledCode,
               return_register: Option<usize>)
               -> ExecutionContext {
        let line = code.line;

        ExecutionContext {
            register: Register::new(),
            binding: binding,
            code: code,
            parent: None,
            instruction_index: 0,
            return_register: return_register,
            line: line,
        }
    }

    /// Returns a new ExecutionContext with a parent binding.
    pub fn with_binding(parent_binding: RcBinding,
                        code: RcCompiledCode,
                        return_register: Option<usize>)
                        -> ExecutionContext {
        let binding = Binding::with_parent(parent_binding, code.locals as usize);

        ExecutionContext::new(binding, code, return_register)
    }

    pub fn file(&self) -> &String {
        &self.code.file
    }

    pub fn name(&self) -> &String {
        &self.code.name
    }

    pub fn set_parent(&mut self, parent: Box<ExecutionContext>) {
        self.parent = Some(parent);
    }

    pub fn parent(&self) -> Option<&Box<ExecutionContext>> {
        self.parent.as_ref()
    }

    pub fn parent_mut(&mut self) -> Option<&mut Box<ExecutionContext>> {
        self.parent.as_mut()
    }

    pub fn get_register(&self, register: usize) -> ObjectPointer {
        self.register.get(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.register.set(register, value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        self.binding.get_local(index)
    }

    pub fn set_local(&mut self, index: usize, value: ObjectPointer) {
        self.binding.set_local(index, value);
    }

    pub fn binding(&self) -> RcBinding {
        self.binding.clone()
    }

    /// Finds a parent context at most `depth` contexts up the ancestor chain.
    ///
    /// For example, using a `depth` of 2 means this method will at most
    /// traverse 2 parent contexts.
    pub fn find_parent(&self, depth: usize) -> Option<&Box<ExecutionContext>> {
        let mut found = self.parent();

        for _ in 0..(depth - 1) {
            if let Some(unwrapped) = found {
                found = unwrapped.parent();
            } else {
                return None;
            }
        }

        found
    }

    /// Returns an iterator for traversing the context chain, including the
    /// current context.
    pub fn contexts(&self) -> ExecutionContextIterator {
        ExecutionContextIterator { current: Some(self) }
    }

    /// Returns pointers to all pointers stored in this context.
    pub fn pointers(&self) -> Vec<ObjectPointerPointer> {
        self.binding.pointers().chain(self.register.pointers()).collect()
    }
}

impl<'a> Iterator for ExecutionContextIterator<'a> {
    type Item = &'a ExecutionContext;

    fn next(&mut self) -> Option<&'a ExecutionContext> {
        if let Some(ctx) = self.current {
            if let Some(parent) = ctx.parent() {
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
    use compiled_code::{CompiledCode, RcCompiledCode};
    use object_pointer::{ObjectPointer, RawObjectPointer};
    use binding::Binding;

    fn new_compiled_code() -> RcCompiledCode {
        CompiledCode::with_rc("a".to_string(),
                              "a.inko".to_string(),
                              1,
                              Vec::new())
    }

    fn new_context() -> ExecutionContext {
        ExecutionContext::new(Binding::new(), new_compiled_code(), None)
    }

    #[test]
    fn test_new() {
        let binding = Binding::new();
        let code = new_compiled_code();
        let context = ExecutionContext::new(binding, code, Some(4));

        assert!(context.parent.is_none());
        assert_eq!(context.instruction_index, 0);

        assert!(context.return_register.is_some());
        assert_eq!(context.return_register.unwrap(), 4);
    }

    #[test]
    fn test_with_object() {
        let code = new_compiled_code();
        let context = ExecutionContext::new(Binding::new(), code, Some(4));

        assert!(context.parent.is_none());
        assert_eq!(context.instruction_index, 0);

        assert!(context.return_register.is_some());
        assert_eq!(context.return_register.unwrap(), 4);
    }

    #[test]
    fn test_with_binding() {
        let binding = Binding::new();
        let code = new_compiled_code();
        let context = ExecutionContext::with_binding(binding, code, None);

        assert!(context.binding.parent().is_some());
    }

    #[test]
    fn test_set_parent() {
        let binding = Binding::new();
        let code = new_compiled_code();
        let context1 = ExecutionContext::new(binding.clone(), code.clone(), None);
        let mut context2 = ExecutionContext::new(binding, code, None);

        context2.set_parent(Box::new(context1));

        assert!(context2.parent.is_some());
    }

    #[test]
    fn test_parent_without_parent() {
        let binding = Binding::new();
        let code = new_compiled_code();
        let mut context =
            ExecutionContext::new(binding.clone(), code.clone(), None);

        assert!(context.parent().is_none());
        assert!(context.parent_mut().is_none());
    }

    #[test]
    fn test_parent_with_parent() {
        let binding = Binding::new();
        let code = new_compiled_code();
        let context1 = ExecutionContext::new(binding.clone(), code.clone(), None);
        let mut context2 = ExecutionContext::new(binding, code, None);

        context2.set_parent(Box::new(context1));

        assert!(context2.parent().is_some());
        assert!(context2.parent_mut().is_some());
    }

    #[test]
    #[should_panic]
    fn test_get_register_invalid() {
        let context = new_context();

        context.get_register(0);
    }

    #[test]
    fn test_get_set_register_valid() {
        let mut context = new_context();
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        context.set_register(0, pointer);

        assert!(context.get_register(0) == pointer);
    }

    #[test]
    fn test_get_local_invalid() {
        let context = new_context();

        assert!(context.get_local(0).is_err());
    }

    #[test]
    fn test_get_set_local_valid() {
        let mut context = new_context();
        let pointer = ObjectPointer::null();

        context.set_local(0, pointer);

        assert!(context.get_local(0).is_ok());
    }

    #[test]
    fn test_find_parent() {
        let context1 = new_context();
        let mut context2 = new_context();
        let mut context3 = new_context();

        context2.set_parent(Box::new(context1));
        context3.set_parent(Box::new(context2));

        let found = context3.find_parent(1);

        assert!(found.is_some());
        assert!(found.unwrap().parent().is_some());
        assert!(found.unwrap().parent().unwrap().parent().is_none());
    }

    #[test]
    fn test_contexts() {
        let context1 = new_context();
        let mut context2 = new_context();
        let mut context3 = new_context();

        context2.set_parent(Box::new(context1));
        context3.set_parent(Box::new(context2));

        let mut contexts = context3.contexts();

        assert!(contexts.next().is_some());
        assert!(contexts.next().is_some());
        assert!(contexts.next().is_some());
        assert!(contexts.next().is_none());
    }

    #[test]
    fn test_pointers() {
        let mut context = new_context();
        let pointer = ObjectPointer::new(0x1 as RawObjectPointer);

        context.register.set(0, pointer);
        context.binding.set_local(0, pointer);

        let pointers = context.pointers();

        assert_eq!(pointers.len(), 2);
    }
}
