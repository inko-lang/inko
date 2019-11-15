//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.
use crate::binding::{Binding, RcBinding};
use crate::block::Block;
use crate::compiled_code::CompiledCodePointer;
use crate::global_scope::GlobalScopePointer;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::process::RcProcess;
use crate::register::Register;

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
    pub return_register: Option<u16>,

    /// The current line that is being executed.
    pub line: u16,

    /// The current global scope.
    pub global_scope: GlobalScopePointer,

    /// If a process should terminate once it returns from this context.
    pub terminate_upon_return: bool,

    /// Blocks to execute when returning from this context.
    pub deferred_blocks: Vec<ObjectPointer>,
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
    pub fn from_block(
        block: &Block,
        return_register: Option<u16>,
    ) -> ExecutionContext {
        ExecutionContext {
            register: Register::new(block.code.registers as usize),
            binding: Binding::from_block(block),
            deferred_blocks: Vec::new(),
            code: block.code,
            parent: None,
            instruction_index: 0,
            return_register,
            line: block.code.line,
            global_scope: block.global_scope,
            terminate_upon_return: false,
        }
    }

    pub fn from_isolated_block(block: &Block) -> ExecutionContext {
        ExecutionContext {
            register: Register::new(block.code.registers as usize),
            binding: Binding::with_rc(block.locals(), block.receiver),
            code: block.code,
            deferred_blocks: Vec::new(),
            parent: None,
            instruction_index: 0,
            return_register: None,
            line: block.code.line,
            global_scope: block.global_scope,
            terminate_upon_return: false,
        }
    }

    pub fn file(&self) -> ObjectPointer {
        self.code.file
    }

    pub fn name(&self) -> ObjectPointer {
        self.code.name
    }

    pub fn set_parent(&mut self, parent: Box<ExecutionContext>) {
        self.parent = Some(parent);
    }

    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box))]
    pub fn parent(&self) -> Option<&Box<ExecutionContext>> {
        self.parent.as_ref()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box))]
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
    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box))]
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
        ExecutionContextIterator {
            current: Some(self),
        }
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        self.binding.each_pointer(|ptr| callback(ptr));
        self.register.each_pointer(|ptr| callback(ptr));

        for pointer in &self.deferred_blocks {
            callback(pointer.pointer());
        }
    }

    /// Returns the top-most parent binding of the current binding.
    pub fn top_binding_pointer(&self) -> *const Binding {
        let mut current = self.binding();

        while let Some(parent) = current.parent() {
            current = parent;
        }

        &*current as *const Binding
    }

    pub fn binding_pointer(&self) -> *const Binding {
        &*self.binding as *const Binding
    }

    pub fn terminate_upon_return(&mut self) {
        self.terminate_upon_return = true;
    }

    pub fn add_defer(&mut self, block: ObjectPointer) {
        self.deferred_blocks.push(block);
    }

    /// Schedules all the deferred blocks of the current context.
    ///
    /// The OK value of this method is a boolean indicating if any blocks were
    /// scheduled.
    pub fn schedule_deferred_blocks(
        &mut self,
        process: &RcProcess,
    ) -> Result<bool, String> {
        if self.deferred_blocks.is_empty() {
            return Ok(false);
        }

        for pointer in self.deferred_blocks.drain(0..) {
            let block = pointer.block_value()?;

            process.push_context(Self::from_block(block, None));
        }

        Ok(true)
    }

    /// Schedules all deferred blocks in all parent contexts.
    ///
    /// The OK value of this method is a boolean indicating if any blocks were
    /// scheduled.
    pub fn schedule_deferred_blocks_of_all_parents(
        &mut self,
        process: &RcProcess,
    ) -> Result<bool, String> {
        let mut current = self.parent_mut();
        let mut scheduled = false;

        while let Some(context) = current {
            if context.schedule_deferred_blocks(process)? {
                scheduled = true;
            }

            current = context.parent_mut();
        }

        Ok(scheduled)
    }

    pub fn append_deferred_blocks(&mut self, source: &mut Vec<ObjectPointer>) {
        if source.is_empty() {
            return;
        }

        self.deferred_blocks.append(source);
    }

    pub fn move_deferred_blocks_to(&mut self, target: &mut Vec<ObjectPointer>) {
        if self.deferred_blocks.is_empty() {
            return;
        }

        target.append(&mut self.deferred_blocks);
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
    use crate::object_pointer::{ObjectPointer, RawObjectPointer};
    use crate::vm::test::*;
    use std::mem;

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
    fn test_each_pointer() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block, None);
        let pointer = ObjectPointer::new(0x1 as RawObjectPointer);
        let deferred = ObjectPointer::integer(5);

        context.register.set(0, pointer);
        context.binding.set_local(0, pointer);
        context.add_defer(deferred);

        let mut pointer_pointers = Vec::new();

        context.each_pointer(|ptr| pointer_pointers.push(ptr));

        let pointers: Vec<_> =
            pointer_pointers.into_iter().map(|x| *x.get()).collect();

        assert_eq!(pointers.len(), 4);
        assert_eq!(pointers.iter().filter(|x| **x == pointer).count(), 2);

        assert!(pointers.contains(&context.binding.receiver));
        assert!(pointers.contains(&deferred));
    }

    #[test]
    fn test_type_size() {
        let size = mem::size_of::<ExecutionContext>();

        // This test is put in place to ensure the type size doesn't change
        // unintentionally.
        assert_eq!(size, 88);
    }

    #[test]
    fn test_append_deferred_blocks() {
        let (_machine, block, _) = setup();
        let pointer = ObjectPointer::integer(5);
        let mut context = ExecutionContext::from_block(&block, None);
        let mut pointers = vec![pointer];

        context.append_deferred_blocks(&mut pointers);

        assert!(pointers.is_empty());
        assert!(context.deferred_blocks[0] == pointer);
    }

    #[test]
    fn test_move_deferred_blocks_to() {
        let (_machine, block, _) = setup();
        let pointer = ObjectPointer::integer(5);
        let mut context = ExecutionContext::from_block(&block, None);
        let mut pointers = Vec::new();

        context.deferred_blocks.push(pointer);
        context.move_deferred_blocks_to(&mut pointers);

        assert!(context.deferred_blocks.is_empty());
        assert!(pointers[0] == pointer);
    }
}
