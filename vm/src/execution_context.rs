//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.

use binding::{Binding, RcBinding};
use block::Block;
use compiled_code::CompiledCodePointer;
use global_scope::GlobalScopePointer;
use object_pointer::{ObjectPointer, ObjectPointerPointer};
use register::Register;

pub struct ExecutionContext {
    /// The registers for this context.
    pub register: Register,

    /// The binding to evaluate this context in.
    pub binding: RcBinding,

    /// The CompiledCodea object associated with this context.
    pub code: CompiledCodePointer,

    /// The parent execution context.
    pub parent: Option<Box<ExecutionContext>>,

    /// The index of the instruction to store prior to suspending a process.
    pub instruction_index: usize,

    /// The register to store this context's return value in.
    pub return_register: Option<usize>,

    /// The current line that is being executed.
    pub line: u16,

    /// The current global scope.
    pub global_scope: GlobalScopePointer,
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
    /// Creates a new execution context using an existing bock.
    #[inline(always)]
    pub fn from_block(block: &Block,
                      return_register: Option<usize>)
                      -> ExecutionContext {
        ExecutionContext {
            register: Register::new(block.code.registers as usize),
            binding: Binding::from_block(block),
            code: block.code,
            parent: None,
            instruction_index: 0,
            return_register: return_register,
            line: block.code.line,
            global_scope: block.global_scope,
        }
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

    pub fn get_local(&self, index: usize) -> ObjectPointer {
        self.binding.get_local(index)
    }

    pub fn set_local(&mut self, index: usize, value: ObjectPointer) {
        self.binding.set_local(index, value);
    }

    pub fn get_global(&self, index: usize) -> ObjectPointer {
        self.global_scope.get(index)
    }

    pub fn set_global(&mut self, index: usize, value: ObjectPointer) {
        self.global_scope.set(index, value);
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
        self.binding
            .pointers()
            .chain(self.register.pointers())
            .collect()
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
    use object_pointer::{ObjectPointer, RawObjectPointer};
    use vm::test::*;

    #[test]
    fn test_set_parent() {
        let (_machine, block, _) = setup();
        let context1 = ExecutionContext::from_block(&block, None);
        let mut context2 = ExecutionContext::from_block(&block, None);

        context2.set_parent(Box::new(context1));

        assert!(context2.parent.is_some());
    }

    #[test]
    fn test_parent_without_parent() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block, None);

        assert!(context.parent().is_none());
        assert!(context.parent_mut().is_none());
    }

    #[test]
    fn test_get_set_register_valid() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block, None);
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        context.set_register(0, pointer);

        assert!(context.get_register(0) == pointer);
    }

    #[test]
    fn test_get_set_local_valid() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block, None);
        let pointer = ObjectPointer::null();

        context.set_local(0, pointer);

        assert!(context.get_local(0) == pointer);
    }

    #[test]
    fn test_find_parent() {
        let (_machine, block, _) = setup();

        let context1 = ExecutionContext::from_block(&block, None);
        let mut context2 = ExecutionContext::from_block(&block, None);
        let mut context3 = ExecutionContext::from_block(&block, None);

        context2.set_parent(Box::new(context1));
        context3.set_parent(Box::new(context2));

        let found = context3.find_parent(1);

        assert!(found.is_some());
        assert!(found.unwrap().parent().is_some());
        assert!(found.unwrap().parent().unwrap().parent().is_none());
    }

    #[test]
    fn test_contexts() {
        let (_machine, block, _) = setup();

        let context1 = ExecutionContext::from_block(&block, None);
        let mut context2 = ExecutionContext::from_block(&block, None);
        let mut context3 = ExecutionContext::from_block(&block, None);

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
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block, None);
        let pointer = ObjectPointer::new(0x1 as RawObjectPointer);

        context.register.set(0, pointer);
        context.binding.set_local(0, pointer);

        let pointers = context.pointers();

        assert_eq!(pointers.len(), 2);
    }
}
