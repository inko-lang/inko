//! Variable Bindings
//!
//! A Binding is a structure containing information about the variables (e.g.
//! local variables and "self") of a certain execution context.

use std::sync::Arc;
use std::cell::UnsafeCell;

use object_pointer::ObjectPointer;

pub struct Binding {
    /// The object "self" refers to.
    pub self_object: ObjectPointer,

    /// The local variables in the current binding.
    ///
    /// Local variables must **not** be modified concurrently as access is not
    /// synchronized due to 99% of all operations being process-local.
    pub locals: UnsafeCell<Vec<ObjectPointer>>,

    /// The parent binding, if any.
    pub parent: Option<RcBinding>,
}

pub type RcBinding = Arc<Binding>;

impl Binding {
    /// Returns a new binding.
    pub fn new(self_object: ObjectPointer) -> RcBinding {
        let bind = Binding {
            self_object: self_object,
            locals: UnsafeCell::new(Vec::new()),
            parent: None,
        };

        Arc::new(bind)
    }

    /// Returns a new binding with a parent binding.
    pub fn with_parent(self_object: ObjectPointer,
                       parent_binding: RcBinding)
                       -> RcBinding {
        let bind = Binding {
            self_object: self_object,
            locals: UnsafeCell::new(Vec::new()),
            parent: Some(parent_binding),
        };

        Arc::new(bind)
    }

    /// Returns the value of a local variable.
    pub fn get_local(&self, index: usize) -> Result<ObjectPointer, String> {
        self.locals()
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Undefined local variable index {}", index))
    }

    /// Sets a local variable.
    pub fn set_local(&self, index: usize, value: ObjectPointer) {
        self.locals_mut().insert(index, value);
    }

    /// Returns true if the local variable exists.
    pub fn local_exists(&self, index: usize) -> bool {
        self.locals().get(index).is_some()
    }

    /// Returns the parent binding.
    pub fn parent(&self) -> Option<RcBinding> {
        self.parent.clone()
    }

    /// Returns a pointer to the "self" object.
    pub fn self_object(&self) -> ObjectPointer {
        self.self_object.clone()
    }

    /// Tries to find a parent binding while limiting the amount of bindings to
    /// traverse.
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

    /// Returns an immutable reference to this binding's local variables.
    pub fn locals(&self) -> &Vec<ObjectPointer> {
        unsafe { &*self.locals.get() }
    }

    /// Returns a mutable reference to this binding's local variables.
    pub fn locals_mut(&self) -> &mut Vec<ObjectPointer> {
        unsafe { &mut *self.locals.get() }
    }

    /// Returns a collection of pointers to the pointers reachable by this
    /// binding.
    pub fn pointers(&self) -> Vec<*const ObjectPointer> {
        let mut pointers = Vec::new();

        self.push_pointers(&mut pointers, self);

        let mut parent_opt = self.parent.as_ref();

        while let Some(parent) = parent_opt {
            self.push_pointers(&mut pointers, &*parent);

            parent_opt = parent.parent.as_ref();
        }

        pointers
    }

    fn push_pointers(&self,
                     pointers: &mut Vec<*const ObjectPointer>,
                     binding: &Binding) {
        pointers.push(binding.self_object.as_raw_pointer());

        for local in binding.locals().iter() {
            pointers.push(local.as_raw_pointer());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{ObjectPointer, RawObjectPointer};

    #[test]
    fn test_with_parent() {
        let ptr = ObjectPointer::null();
        let binding1 = Binding::new(ptr);
        let binding2 = Binding::with_parent(ptr, binding1.clone());

        assert!(binding2.parent.is_some());
    }

    #[test]
    fn test_get_local_invalid() {
        let binding = Binding::new(ObjectPointer::null());

        assert!(binding.get_local(0).is_err());
    }

    #[test]
    fn test_get_local_valid() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        binding.set_local(0, ptr);

        assert!(binding.get_local(0).is_ok());
    }

    #[test]
    fn test_set_local() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_local_exists_non_existing_local() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        assert_eq!(binding.local_exists(0), false);
    }

    #[test]
    fn test_local_exists_existing_local() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        binding.set_local(0, ptr);

        assert!(binding.local_exists(0));
    }

    #[test]
    fn test_parent_without_parent() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        assert!(binding.parent().is_none());
    }

    #[test]
    fn test_parent_with_parent() {
        let ptr = ObjectPointer::null();
        let binding1 = Binding::new(ptr);
        let binding2 = Binding::with_parent(ptr, binding1);

        assert!(binding2.parent().is_some());
    }

    #[test]
    fn test_find_parent_without_parent() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        assert!(binding.find_parent(1).is_none());
    }

    #[test]
    fn test_find_parent_with_parent() {
        let ptr = ObjectPointer::null();
        let binding1 = Binding::new(ptr);
        let binding2 = Binding::with_parent(ptr, binding1);
        let binding3 = Binding::with_parent(ptr, binding2);
        let binding4 = Binding::with_parent(ptr, binding3);

        let found = binding4.find_parent(1);

        assert!(found.is_some());
        assert!(found.unwrap().parent.is_some());
    }

    #[test]
    fn test_locals() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals_mut() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new(ptr);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals_mut().len(), 1);
    }

    #[test]
    fn test_pointers() {
        let self_obj1 = ObjectPointer::new(0x1 as RawObjectPointer);
        let local1 = ObjectPointer::new(0x2 as RawObjectPointer);
        let binding1 = Binding::new(self_obj1);

        binding1.set_local(0, local1);

        let self_obj2 = ObjectPointer::new(0x3 as RawObjectPointer);
        let local2 = ObjectPointer::new(0x4 as RawObjectPointer);
        let binding2 = Binding::with_parent(self_obj2, binding1.clone());

        binding2.set_local(0, local2);

        let pointers = binding2.pointers();

        assert_eq!(pointers.len(), 4);

        assert!(unsafe { *pointers[0] } == self_obj2);
        assert!(unsafe { *pointers[1] } == local2);

        assert!(unsafe { *pointers[2] } == self_obj1);
        assert!(unsafe { *pointers[3] } == local1);
    }
}
