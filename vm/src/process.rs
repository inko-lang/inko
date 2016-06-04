use std::mem;

use call_frame::CallFrame;
use compiled_code::RcCompiledCode;
use heap::Heap;
use object::Object;
use object_pointer::ObjectPointer;
use object_value;
use std::sync::{Arc, RwLock};

const TICK_COUNT: usize = 2000;

pub type RcProcess = Arc<RwLock<Process>>;

pub enum ProcessStatus {
    Scheduled = 1,
    Running = 2,
    Paused = 3,
    Errored = 4,
    Finished = 5
}

pub struct Process {
    pub pid: usize,
    pub eden_heap: Heap,
    pub young_heap: Heap,
    pub mature_heap: Heap,
    pub status: ProcessStatus,
    pub compiled_code: RcCompiledCode,
    pub call_frame: CallFrame,
    pub ticks: usize
}

impl Process {
    pub fn new(pid: usize, call_frame: CallFrame, code: RcCompiledCode) -> RcProcess {
        let task = Process {
            pid: pid,
            eden_heap: Heap::new(),
            young_heap: Heap::new(),
            mature_heap: Heap::new(),
            status: ProcessStatus::Scheduled,
            compiled_code: code,
            call_frame: call_frame,
            ticks: TICK_COUNT
        };

        Arc::new(RwLock::new(task))
    }

    pub fn from_code(pid: usize, code: RcCompiledCode, self_obj: ObjectPointer) -> RcProcess {
        let frame = CallFrame::from_code(code.clone(), self_obj);

        Process::new(pid, frame, code)
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

    pub fn get_register(&self, register: usize) -> Result<ObjectPointer, String> {
        self.call_frame.register
            .get(register)
            .ok_or_else(|| format!("Undefined object in register {}", register))
    }

    pub fn get_register_option(&self, register: usize) -> Option<ObjectPointer> {
        self.call_frame.register.get(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.call_frame.register.set(register, value);
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        let mut binding = write_lock!(self.call_frame.binding);

        binding.variables.insert(index, value);
    }

    pub fn add_local(&self, value: ObjectPointer) {
        let mut binding = write_lock!(self.call_frame.binding);

        binding.variables.push(value);
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        let binding = read_lock!(self.call_frame.binding);

        binding.variables
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    pub fn local_exists(&self, index: usize) -> bool {
        let binding = read_lock!(self.call_frame.binding);

        binding.variables.get(index).is_some()
    }

    pub fn should_pause(&self) -> bool {
        self.ticks == 0
    }

    pub fn allocate_empty(&mut self) -> ObjectPointer {
        let obj = Object::new(object_value::none());

        self.eden_heap.allocate_local(obj)
    }

    pub fn allocate(&mut self, value: object_value::ObjectValue,
                    proto: ObjectPointer) -> ObjectPointer {
        let obj = Object::with_prototype(value, proto);

        self.eden_heap.allocate_local(obj)
    }

    pub fn allocate_without_prototype(&mut self, value: object_value::ObjectValue) -> ObjectPointer {
        let obj = Object::new(value);

        self.eden_heap.allocate_local(obj)
    }
}
