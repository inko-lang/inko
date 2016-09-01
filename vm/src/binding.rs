use std::sync::{Arc, RwLock};

use object_pointer::ObjectPointer;

pub struct Binding {
    pub self_object: ObjectPointer,
    pub locals: RwLock<Vec<ObjectPointer>>,
    pub parent: Option<RcBinding>,
}

pub type RcBinding = Arc<Binding>;

impl Binding {
    pub fn new(self_object: ObjectPointer) -> RcBinding {
        let bind = Binding {
            self_object: self_object,
            locals: RwLock::new(Vec::new()),
            parent: None,
        };

        Arc::new(bind)
    }

    pub fn with_parent(self_object: ObjectPointer,
                       parent_binding: RcBinding)
                       -> RcBinding {
        let bind = Binding {
            self_object: self_object,
            locals: RwLock::new(Vec::new()),
            parent: Some(parent_binding),
        };

        Arc::new(bind)
    }

    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        read_lock!(self.locals)
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        write_lock!(self.locals).insert(index, value);
    }

    pub fn local_exists(&self, index: usize) -> bool {
        read_lock!(self.locals).get(index).is_some()
    }

    pub fn parent(&self) -> Option<RcBinding> {
        self.parent.clone()
    }

    pub fn self_object(&self) -> ObjectPointer {
        self.self_object.clone()
    }

    pub fn find_parent(&self, depth: usize) -> Option<RcBinding> {
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

    pub fn each_binding<F>(&self, mut closure: F)
        where F: FnMut(&Self)
    {
        let mut binding = self;

        closure(binding);

        while binding.parent.is_some() {
            binding = binding.parent.as_ref().unwrap();

            closure(binding);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
