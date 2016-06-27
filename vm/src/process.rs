use std::mem;
use std::sync::{Arc, Mutex, MutexGuard};
use std::cell::UnsafeCell;

use binding::RcBinding;
use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use heap::Heap;
use mailbox::Mailbox;
use object::Object;
use object_pointer::ObjectPointer;
use object_value;
use execution_context::ExecutionContext;

pub type RcProcess = Arc<Process>;

pub enum ProcessStatus {
    Scheduled,
    Running,
    Suspended,
    Failed,
}

pub struct LocalData {
    pub status: ProcessStatus,
    pub eden_heap: Heap,
    pub young_heap: Option<Box<Heap>>,
    pub mature_heap: Option<Box<Heap>>,
    pub call_frame: CallFrame,
    pub context: ExecutionContext,
}

pub struct Process {
    pub pid: usize,
    pub mailbox: Mutex<Option<Box<Mailbox>>>,
    pub local_data: UnsafeCell<LocalData>,
}

unsafe impl Sync for LocalData {}
unsafe impl Sync for Process {}

impl Process {
    pub fn new(pid: usize,
               call_frame: CallFrame,
               context: ExecutionContext)
               -> RcProcess {
        let local_data = LocalData {
            eden_heap: Heap::local(),
            young_heap: None,
            mature_heap: None,
            call_frame: call_frame,
            context: context,
            status: ProcessStatus::Scheduled,
        };

        let process = Process {
            pid: pid,
            local_data: UnsafeCell::new(local_data),
            mailbox: Mutex::new(None),
        };

        Arc::new(process)
    }

    pub fn from_code(pid: usize,
                     code: RcCompiledCode,
                     self_obj: ObjectPointer)
                     -> RcProcess {
        let frame = CallFrame::from_code(code.clone());
        let context = ExecutionContext::with_object(self_obj, code, None);

        Process::new(pid, frame, context)
    }

    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn push_call_frame(&self, mut frame: CallFrame) {
        let mut local_data = self.local_data_mut();
        let ref mut target = local_data.call_frame;

        mem::swap(target, &mut frame);

        target.set_parent(frame);
    }

    pub fn pop_call_frame(&self) {
        let mut local_data = self.local_data_mut();

        if local_data.call_frame.parent.is_none() {
            return;
        }

        let parent = local_data.call_frame.parent.take().unwrap();

        local_data.call_frame = *parent;
    }

    pub fn push_context(&self, mut context: ExecutionContext) {
        let mut local_data = self.local_data_mut();
        let ref mut target = local_data.context;

        mem::swap(target, &mut context);

        target.set_parent(context);
    }

    pub fn pop_context(&self) {
        let mut local_data = self.local_data_mut();

        if local_data.context.parent.is_none() {
            return;
        }

        let parent = local_data.context.parent.take().unwrap();

        local_data.context = *parent;
    }

    pub fn get_register(&self, register: usize) -> Result<ObjectPointer, String> {
        self.local_data()
            .context
            .get_register(register)
            .ok_or_else(|| format!("Undefined object in register {}", register))
    }

    pub fn get_register_option(&self, register: usize) -> Option<ObjectPointer> {
        self.local_data().context.get_register(register)
    }

    pub fn set_register(&self, register: usize, value: ObjectPointer) {
        self.local_data_mut().context.set_register(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        let local_data = self.local_data();
        let mut binding = write_lock!(local_data.context.binding);

        binding.variables.insert(index, value);
    }

    pub fn add_local(&self, value: ObjectPointer) {
        let local_data = self.local_data();
        let mut binding = write_lock!(local_data.context.binding);

        binding.variables.push(value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        let local_data = self.local_data();
        let binding = read_lock!(local_data.context.binding);

        binding.variables
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let local_data = self.local_data();
        let binding = read_lock!(local_data.context.binding);

        binding.variables.get(index).is_some()
    }

    pub fn allocate_empty(&self) -> ObjectPointer {
        self.local_data_mut().eden_heap.allocate_empty()
    }

    pub fn allocate(&self,
                    value: object_value::ObjectValue,
                    proto: ObjectPointer)
                    -> ObjectPointer {
        let mut local_data = self.local_data_mut();

        local_data.eden_heap.allocate_value_with_prototype(value, proto)
    }

    pub fn allocate_without_prototype(&self,
                                      value: object_value::ObjectValue)
                                      -> ObjectPointer {
        let mut local_data = self.local_data_mut();

        local_data.eden_heap.allocate(Object::new(value))
    }

    pub fn send_message(&self, message: ObjectPointer) {
        let mut mailbox = self.mailbox();

        mailbox.as_mut().unwrap().send(message);
    }

    pub fn receive_message(&self) -> Option<ObjectPointer> {
        let mut mailbox = self.mailbox();

        mailbox.as_mut().unwrap().receive()
    }

    pub fn mailbox<'a>(&'a self) -> MutexGuard<'a, Option<Box<Mailbox>>> {
        let mut mailbox = unlock!(self.mailbox);

        let allocate = if mailbox.is_none() {
            true
        } else {
            false
        };

        if allocate {
            *mailbox = Some(Box::new(Mailbox::new()));
        }

        mailbox
    }

    pub fn is_suspended(&self) -> bool {
        match self.local_data().status {
            ProcessStatus::Suspended => true,
            _ => false,
        }
    }

    /// Adds a new call frame pointing to the given line number.
    pub fn advance_line(&self, line: u32) {
        let frame = CallFrame::new(self.compiled_code(), line);

        self.push_call_frame(frame);
    }

    pub fn binding(&self) -> RcBinding {
        self.context().binding.clone()
    }

    pub fn self_object(&self) -> ObjectPointer {
        self.context().self_object()
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.local_data().context
    }

    pub fn context_mut(&self) -> &mut ExecutionContext {
        &mut self.local_data_mut().context
    }

    pub fn at_top_level(&self) -> bool {
        self.context().parent.is_none()
    }

    pub fn call_frame(&self) -> &CallFrame {
        &self.local_data().call_frame
    }

    pub fn compiled_code(&self) -> RcCompiledCode {
        self.context().code.clone()
    }

    pub fn instruction_index(&self) -> usize {
        self.context().instruction_index
    }

    pub fn set_instruction_index(&self, index: usize) {
        self.context_mut().instruction_index = index;
    }

    pub fn mark_running(&self) {
        self.local_data_mut().status = ProcessStatus::Running;
    }

    pub fn suspend(&self) {
        self.local_data_mut().status = ProcessStatus::Suspended;
    }
}
