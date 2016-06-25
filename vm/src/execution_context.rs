use binding::{Binding, RcBinding};
use compiled_code::RcCompiledCode;
use object_pointer::ObjectPointer;
use register::Register;

pub struct ExecutionContext {
    pub register: Register,
    pub binding: RcBinding,
    pub code: RcCompiledCode,
    pub parent: Option<Box<ExecutionContext>>,
    pub instruction_index: usize,
    pub return_register: Option<usize>,
}

impl ExecutionContext {
    pub fn new(binding: RcBinding,
               code: RcCompiledCode,
               return_register: Option<usize>)
               -> ExecutionContext {
        ExecutionContext {
            register: Register::new(),
            binding: binding,
            code: code,
            parent: None,
            instruction_index: 0,
            return_register: return_register,
        }
    }

    pub fn with_object(object: ObjectPointer,
                       code: RcCompiledCode,
                       return_register: Option<usize>)
                       -> ExecutionContext {
        ExecutionContext::new(Binding::new(object), code, return_register)
    }

    pub fn set_parent(&mut self, parent: ExecutionContext) {
        self.parent = Some(Box::new(parent));
    }

    pub fn parent(&self) -> Option<&Box<ExecutionContext>> {
        self.parent.as_ref()
    }

    pub fn parent_mut(&mut self) -> Option<&mut Box<ExecutionContext>> {
        self.parent.as_mut()
    }

    pub fn self_object(&self) -> ObjectPointer {
        read_lock!(self.binding).self_object.clone()
    }

    pub fn get_register(&self, register: usize) -> Option<ObjectPointer> {
        self.register.get(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.register.set(register, value);
    }
}
