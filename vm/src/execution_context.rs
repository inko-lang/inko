use binding::{Binding, RcBinding};
use compiled_code::RcCompiledCode;
use object_pointer::ObjectPointer;
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

    pub fn set_parent(&mut self, parent: Box<ExecutionContext>) {
        self.parent = Some(parent);
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

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        self.binding.get_local(index)
    }

    pub fn set_local(&mut self, index: usize, value: ObjectPointer) {
        self.binding.set_local(index, value);
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

    pub fn each_context<F>(&self, mut closure: F)
        where F: FnMut(&Self)
    {
        let mut context = self;

        closure(context);

        while context.parent.is_some() {
            context = context.parent.as_ref().unwrap();

            closure(context);
        }
    }
}
