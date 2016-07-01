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

    pub fn with_binding(parent_binding: RcBinding,
                        code: RcCompiledCode,
                        return_register: Option<usize>)
                        -> ExecutionContext {
        let object = parent_binding.self_object();
        let binding = Binding::with_parent(object, parent_binding);

        ExecutionContext::new(binding, code, return_register)
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
        self.binding.self_object.clone()
    }

    pub fn get_register(&self, register: usize) -> Option<ObjectPointer> {
        self.register.get(register)
    }

    pub fn set_register(&mut self, register: usize, value: ObjectPointer) {
        self.register.set(register, value);
    }

    pub fn binding(&self) -> RcBinding {
        self.binding.clone()
    }

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
}
