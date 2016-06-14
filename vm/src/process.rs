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
use scope::Scope;

const REDUCTION_COUNT: usize = 2000;

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
    pub compiled_code: RcCompiledCode,
    pub call_frame: CallFrame,
    pub scope: Scope,
    pub reductions: usize,
    pub inbox: Option<RcInbox>,
}

impl Process {
    pub fn new(pid: usize,
               call_frame: CallFrame,
               scope: Scope,
               code: RcCompiledCode)
               -> RcProcess {
        let task = Process {
            pid: pid,
            eden_heap: Heap::local(),
            young_heap: None,
            mature_heap: None,
            status: ProcessStatus::Scheduled,
            compiled_code: code,
            call_frame: call_frame,
            scope: scope,
            reductions: REDUCTION_COUNT,
            inbox: None,
        };

        Arc::new(RwLock::new(task))
    }

    pub fn from_code(pid: usize,
                     code: RcCompiledCode,
                     self_obj: ObjectPointer)
                     -> RcProcess {
        let frame = CallFrame::from_code(code.clone());
        let scope = Scope::with_object(self_obj);

        Process::new(pid, frame, scope, code)
    }

    pub fn push_call_frame(&mut self, mut frame: CallFrame) {
        let ref mut target = self.call_frame;

        mem::swap(target, &mut frame);

        target.set_parent(frame);
    }

    pub fn pop_call_frame(&mut self) {
        let parent = self.call_frame.parent.take().unwrap();

        self.call_frame = *parent;
    }

    /// Pushes a new scope and call frame onto the process.
    pub fn push_scope(&mut self, frame: CallFrame, mut scope: Scope) {
        {
            let ref mut target = self.scope;

            mem::swap(target, &mut scope);

            target.set_parent(scope);
        }

        self.push_call_frame(frame);
    }

    /// Pops a scope and call frame from the current process.
    pub fn pop_scope(&mut self) {
        let parent = self.scope.parent.take().unwrap();

        self.scope = *parent;

        self.pop_call_frame();
    }

    pub fn get_register(&self, register: usize) -> Result<ObjectPointer, String> {
        self.scope
            .register
            .get(register)
            .ok_or_else(|| format!("Undefined object in register {}", register))
    }

    pub fn get_register_option(&self, register: usize) -> Option<ObjectPointer> {
        self.scope.register.get(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.scope.register.set(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        let mut binding = write_lock!(self.scope.binding);

        binding.variables.insert(index, value);
    }

    pub fn add_local(&self, value: ObjectPointer) {
        let mut binding = write_lock!(self.scope.binding);

        binding.variables.push(value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        let binding = read_lock!(self.scope.binding);

        binding.variables
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let binding = read_lock!(self.scope.binding);

        binding.variables.get(index).is_some()
    }

    pub fn reductions_exhausted(&self) -> bool {
        self.reductions == 0
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

    pub fn suspended(&self) -> bool {
        match self.status {
            ProcessStatus::Suspended => true,
            _ => false,
        }
    }

    pub fn binding(&self) -> RcBinding {
        self.scope.binding.clone()
    }

    pub fn self_object(&self) -> ObjectPointer {
        self.scope.self_object()
    }
}
