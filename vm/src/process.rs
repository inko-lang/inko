use std::mem;
use std::sync::{Arc, RwLock};

use binding::RcBinding;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use heap::Heap;
use inbox::{Inbox, RcInbox};
use object::Object;
use object_pointer::ObjectPointer;
use object_value;
use execution_context::ExecutionContext;

pub type RcProcess = Arc<RwLock<Process>>;

pub enum ProcessStatus {
    Scheduled,
    Running,
    Suspended,
    Failed,
}

pub struct Process {
    pub pid: usize,
    pub eden_heap: Heap,
    pub young_heap: Option<Box<Heap>>,
    pub mature_heap: Option<Box<Heap>>,
    pub status: ProcessStatus,
    pub call_frame: CallFrame,
    pub context: ExecutionContext,
    pub inbox: Option<RcInbox>,
}

impl Process {
    pub fn new(pid: usize,
               call_frame: CallFrame,
               context: ExecutionContext)
               -> RcProcess {
        let task = Process {
            pid: pid,
            eden_heap: Heap::local(),
            young_heap: None,
            mature_heap: None,
            status: ProcessStatus::Scheduled,
            call_frame: call_frame,
            context: context,
            inbox: None,
        };

        Arc::new(RwLock::new(task))
    }

    pub fn from_code(pid: usize,
                     code: RcCompiledCode,
                     self_obj: ObjectPointer)
                     -> RcProcess {
        let frame = CallFrame::from_code(code.clone());
        let context = ExecutionContext::with_object(self_obj, code, None);

        Process::new(pid, frame, context)
    }

    pub fn push_call_frame(&mut self, mut frame: CallFrame) {
        let ref mut target = self.call_frame;

        mem::swap(target, &mut frame);

        target.set_parent(frame);
    }

    pub fn pop_call_frame(&mut self) {
        if self.call_frame.parent.is_none() {
            return;
        }

        let parent = self.call_frame.parent.take().unwrap();

        self.call_frame = *parent;
    }

    pub fn push_context(&mut self, mut context: ExecutionContext) {
        let ref mut target = self.context;

        mem::swap(target, &mut context);

        target.set_parent(context);
    }

    pub fn pop_context(&mut self) {
        if self.context.parent.is_none() {
            return;
        }

        let parent = self.context.parent.take().unwrap();

        self.context = *parent;
    }

    pub fn get_register(&self, register: usize) -> Result<ObjectPointer, String> {
        self.context
            .get_register(register)
            .ok_or_else(|| format!("Undefined object in register {}", register))
    }

    pub fn get_register_option(&self, register: usize) -> Option<ObjectPointer> {
        self.context.get_register(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.context.set_register(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        let mut binding = write_lock!(self.context.binding);

        binding.variables.insert(index, value);
    }

    pub fn add_local(&self, value: ObjectPointer) {
        let mut binding = write_lock!(self.context.binding);

        binding.variables.push(value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        let binding = read_lock!(self.context.binding);

        binding.variables
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let binding = read_lock!(self.context.binding);

        binding.variables.get(index).is_some()
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        self.eden_heap.allocate_empty()
    }

    pub fn allocate(&mut self,
                    value: object_value::ObjectValue,
                    proto: ObjectPointer)
                    -> ObjectPointer {
        self.eden_heap.allocate_value_with_prototype(value, proto)
    }

    pub fn allocate_without_prototype(&mut self,
                                      value: object_value::ObjectValue)
                                      -> ObjectPointer {
        self.eden_heap.allocate(Object::new(value))
    }

    pub fn copy_object(&mut self, object_ptr: ObjectPointer) -> ObjectPointer {
        self.eden_heap.copy_object(object_ptr)
    }

    pub fn inbox(&mut self) -> RcInbox {
        let allocate = if self.inbox.is_none() {
            true
        } else {
            false
        };

        if allocate {
            self.inbox = Some(Inbox::new());
        }

        self.inbox.as_ref().cloned().unwrap()
    }

    pub fn is_suspended(&self) -> bool {
        match self.status {
            ProcessStatus::Suspended => true,
            _ => false,
        }
    }

    /// Adds a new call frame pointing to the given line number.
    pub fn advance_line(&mut self, line: u32) {
        let frame = CallFrame::new(self.compiled_code(), line);

        self.push_call_frame(frame);
    }

    pub fn binding(&self) -> RcBinding {
        self.context.binding.clone()
    }

    pub fn self_object(&self) -> ObjectPointer {
        self.context.self_object()
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut ExecutionContext {
        &mut self.context
    }

    pub fn at_top_level(&self) -> bool {
        self.context.parent.is_none()
    }

    pub fn compiled_code(&self) -> RcCompiledCode {
        self.context.code.clone()
    }

    pub fn instruction_index(&self) -> usize {
        self.context.instruction_index
    }

    pub fn set_instruction_index(&mut self, index: usize) {
        self.context.instruction_index = index;
    }

    pub fn mark_running(&mut self) {
        self.status = ProcessStatus::Running;
    }

    pub fn suspend(&mut self) {
        self.status = ProcessStatus::Suspended;
    }
}
