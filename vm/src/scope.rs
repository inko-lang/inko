use binding::{Binding, RcBinding};
use object_pointer::ObjectPointer;
use register::Register;

pub struct Scope {
    pub register: Register,
    pub binding: RcBinding,
    pub parent: Option<Box<Scope>>,
}

impl Scope {
    pub fn new(binding: RcBinding) -> Scope {
        Scope {
            register: Register::new(),
            binding: binding,
            parent: None,
        }
    }

    pub fn with_object(object: ObjectPointer) -> Scope {
        Scope::new(Binding::new(object))
    }

    pub fn set_parent(&mut self, parent: Scope) {
        self.parent = Some(Box::new(parent));
    }

    pub fn parent(&self) -> Option<&Box<Scope>> {
        self.parent.as_ref()
    }

    pub fn self_object(&self) -> ObjectPointer {
        read_lock!(self.binding).self_object.clone()
    }
}
