//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.
use crate::binding::{Binding, RcBinding};
use crate::block::Block;
use crate::compiled_code::CompiledCodePointer;
use crate::deref_pointer::DerefPointer;
use crate::module::Module;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::process::RcProcess;
use crate::registers::Registers;

pub struct ExecutionContext {
    /// The registers for this context.
    pub registers: Registers,

    /// The binding to evaluate this context in.
    pub binding: RcBinding,

    /// The CompiledCodea object associated with this context.
    pub code: CompiledCodePointer,

    /// The parent execution context.
    pub parent: Option<Box<ExecutionContext>>,

    /// The index of the instruction to store prior to suspending a process.
    pub instruction_index: usize,

    /// The current module.
    pub module: DerefPointer<Module>,

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
    #[inline(always)]
    pub fn new(block: &Block, binding: RcBinding) -> Self {
        ExecutionContext {
            registers: Registers::new(block.code.registers),
            binding,
            deferred_blocks: Vec::new(),
            code: block.code,
            parent: None,
            instruction_index: 0,
            module: block.module,
        }
    }

    #[inline(always)]
    pub fn from_block(block: &Block) -> Self {
        let captures = block.captures_from.clone();
        let binding = Binding::new(block.locals(), block.receiver, captures);

        Self::new(block, binding)
    }

    #[inline(always)]
    pub fn from_block_with_receiver(
        block: &Block,
        receiver: ObjectPointer,
    ) -> Self {
        let binding =
            Binding::new(block.locals(), receiver, block.captures_from.clone());

        Self::new(block, binding)
    }

    pub fn file(&self) -> ObjectPointer {
        self.code.file
    }

    pub fn line(&self) -> u16 {
        let mut index = self.instruction_index;

        // When entering a new call frame, the instruction index stored points
        // to the instruction to run _after_ returning; not the one that is
        // being run.
        if index > 0 {
            index -= 1;
        }

        self.code.instructions[index].line as u16
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

    pub fn get_register(&self, register: u16) -> ObjectPointer {
        self.registers.get(register)
    }

    pub fn set_register(&mut self, register: u16, value: ObjectPointer) {
        self.registers.set(register, value);
    }

    pub fn get_local(&self, index: u16) -> ObjectPointer {
        self.binding.get_local(index)
    }

    pub fn set_local(&mut self, index: u16, value: ObjectPointer) {
        self.binding.set_local(index, value);
    }

    pub fn get_global(&self, index: u16) -> ObjectPointer {
        self.module.global_scope().get(index)
    }

    pub fn set_global(&mut self, index: u16, value: ObjectPointer) {
        self.module.global_scope_mut().set(index, value);
    }

    pub fn binding(&self) -> &RcBinding {
        &self.binding
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
        self.registers.each_pointer(|ptr| callback(ptr));

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

        &**current as *const Binding
    }

    pub fn binding_pointer(&self) -> *const Binding {
        &*self.binding as *const Binding
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

            process.push_context(Self::from_block(block));
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
        let context1 = ExecutionContext::from_block(&block);
        let mut context2 = ExecutionContext::from_block(&block);

        context2.set_parent(Box::new(context1));

        assert!(context2.parent.is_some());
    }

    #[test]
    fn test_parent_without_parent() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block);

        assert!(context.parent().is_none());
        assert!(context.parent_mut().is_none());
    }

    #[test]
    fn test_get_set_register_valid() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block);
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        context.set_register(0, pointer);

        assert!(context.get_register(0) == pointer);
    }

    #[test]
    fn test_get_set_local_valid() {
        let (_machine, block, _) = setup();
        let mut context = ExecutionContext::from_block(&block);
        let pointer = ObjectPointer::null();

        context.set_local(0, pointer);

        assert!(context.get_local(0) == pointer);
    }

    #[test]
    fn test_find_parent() {
        let (_machine, block, _) = setup();

        let context1 = ExecutionContext::from_block(&block);
        let mut context2 = ExecutionContext::from_block(&block);
        let mut context3 = ExecutionContext::from_block(&block);

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

        let context1 = ExecutionContext::from_block(&block);
        let mut context2 = ExecutionContext::from_block(&block);
        let mut context3 = ExecutionContext::from_block(&block);

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
        let mut context = ExecutionContext::from_block(&block);
        let pointer = ObjectPointer::new(0x1 as RawObjectPointer);
        let deferred = ObjectPointer::integer(5);

        context.registers.set(0, pointer);
        context.binding.set_local(0, pointer);
        context.add_defer(deferred);

        let mut pointer_pointers = Vec::new();

        context.each_pointer(|ptr| pointer_pointers.push(ptr));

        let pointers: Vec<_> =
            pointer_pointers.into_iter().map(|x| *x.get()).collect();

        assert_eq!(pointers.len(), 4);
        assert_eq!(pointers.iter().filter(|x| **x == pointer).count(), 2);

        assert!(pointers.contains(&context.binding.receiver()));
        assert!(pointers.contains(&deferred));
    }

    #[test]
    fn test_type_size() {
        let size = mem::size_of::<ExecutionContext>();

        // This test is put in place to ensure the type size doesn't change
        // unintentionally.
        assert_eq!(size, 80);
    }

    #[test]
    fn test_append_deferred_blocks() {
        let (_machine, block, _) = setup();
        let pointer = ObjectPointer::integer(5);
        let mut context = ExecutionContext::from_block(&block);
        let mut pointers = vec![pointer];

        context.append_deferred_blocks(&mut pointers);

        assert!(pointers.is_empty());
        assert!(context.deferred_blocks[0] == pointer);
    }

    #[test]
    fn test_move_deferred_blocks_to() {
        let (_machine, block, _) = setup();
        let pointer = ObjectPointer::integer(5);
        let mut context = ExecutionContext::from_block(&block);
        let mut pointers = Vec::new();

        context.deferred_blocks.push(pointer);
        context.move_deferred_blocks_to(&mut pointers);

        assert!(context.deferred_blocks.is_empty());
        assert!(pointers[0] == pointer);
    }
}
