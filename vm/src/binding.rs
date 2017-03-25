//! Variable Bindings
//!
//! A binding contains the local variables available to a certain scope.
use std::sync::Arc;
use std::cell::UnsafeCell;

use immix::copy_object::CopyObject;
use object_pointer::{ObjectPointer, ObjectPointerPointer};

pub struct Binding {
    /// The local variables in the current binding.
    ///
    /// Local variables must **not** be modified concurrently as access is not
    /// synchronized due to 99% of all operations being process-local.
    pub locals: UnsafeCell<Vec<ObjectPointer>>,

    /// The parent binding, if any.
    pub parent: Option<RcBinding>,
}

pub struct PointerIterator<'a> {
    binding: &'a Binding,
    local_index: usize,
}

pub type RcBinding = Arc<Binding>;

impl Binding {
    /// Returns a new binding.
    pub fn new() -> RcBinding {
        let bind = Binding {
            locals: UnsafeCell::new(Vec::new()),
            parent: None,
        };

        Arc::new(bind)
    }

    /// Returns a new binding with a parent binding.
    pub fn with_parent(parent_binding: RcBinding) -> RcBinding {
        let bind = Binding {
            locals: UnsafeCell::new(Vec::new()),
            parent: Some(parent_binding),
        };

        Arc::new(bind)
    }

    /// Reserves space for the given number of local variables.
    pub fn reserve_locals(&self, amount: usize) {
        self.locals_mut().reserve_exact(amount);
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
        let mut locals = self.locals_mut();

        if locals.get(index).is_some() {
            // Existing locals should be overwritten without shifting values.
            locals[index] = value;
        } else {
            locals.insert(index, value);
        }
    }

    /// Returns true if the local variable exists.
    pub fn local_exists(&self, index: usize) -> bool {
        self.locals().get(index).is_some()
    }

    /// Returns the parent binding.
    pub fn parent(&self) -> Option<RcBinding> {
        self.parent.clone()
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

    /// Pushes all pointers in this binding into the supplied vector.
    pub fn push_pointers(&self, pointers: &mut Vec<ObjectPointerPointer>) {
        for pointer in self.pointers() {
            pointers.push(pointer);
        }
    }

    /// Returns an iterator for traversing all pointers in this binding.
    pub fn pointers(&self) -> PointerIterator {
        PointerIterator {
            binding: self,
            local_index: 0,
        }
    }

    /// Creates a new binding and recursively copies over all pointers to the
    /// target heap.
    pub fn clone_to<H: CopyObject>(&self, heap: &mut H) -> RcBinding {
        let parent = if let Some(ref bind) = self.parent {
            Some(bind.clone_to(heap))
        } else {
            None
        };

        let locals =
            self.locals().iter().map(|val| heap.copy_object(*val)).collect();

        Arc::new(Binding {
            locals: UnsafeCell::new(locals),
            parent: parent,
        })
    }
}

impl<'a> Iterator for PointerIterator<'a> {
    type Item = ObjectPointerPointer;

    fn next(&mut self) -> Option<ObjectPointerPointer> {
        loop {
            if let Some(local) = self.binding.locals().get(self.local_index) {
                self.local_index += 1;

                return Some(local.pointer());
            }

            if self.binding.parent.is_some() {
                self.binding = self.binding.parent.as_ref().unwrap();
                self.local_index = 0;
            } else {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{ObjectPointer, RawObjectPointer};
    use object_value;
    use immix::global_allocator::GlobalAllocator;
    use immix::local_allocator::LocalAllocator;

    #[test]
    fn test_with_parent() {
        let binding1 = Binding::new();
        let binding2 = Binding::with_parent(binding1.clone());

        assert!(binding2.parent.is_some());
    }

    #[test]
    fn test_reserve_locals() {
        let binding = Binding::new();

        binding.reserve_locals(4);

        assert_eq!(binding.locals().capacity(), 4);
    }

    #[test]
    fn test_get_local_invalid() {
        let binding = Binding::new();

        assert!(binding.get_local(0).is_err());
    }

    #[test]
    fn test_get_local_valid() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new();

        binding.set_local(0, ptr);

        assert!(binding.get_local(0).is_ok());
    }

    #[test]
    fn test_set_local() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new();

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_local_exists_non_existing_local() {
        let binding = Binding::new();

        assert_eq!(binding.local_exists(0), false);
    }

    #[test]
    fn test_local_exists_existing_local() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new();

        binding.set_local(0, ptr);

        assert!(binding.local_exists(0));
    }

    #[test]
    fn test_parent_without_parent() {
        let binding = Binding::new();

        assert!(binding.parent().is_none());
    }

    #[test]
    fn test_parent_with_parent() {
        let binding1 = Binding::new();
        let binding2 = Binding::with_parent(binding1);

        assert!(binding2.parent().is_some());
    }

    #[test]
    fn test_find_parent_without_parent() {
        let binding = Binding::new();

        assert!(binding.find_parent(1).is_none());
    }

    #[test]
    fn test_find_parent_with_parent() {
        let binding1 = Binding::new();
        let binding2 = Binding::with_parent(binding1);
        let binding3 = Binding::with_parent(binding2);
        let binding4 = Binding::with_parent(binding3);

        let found = binding4.find_parent(1);

        assert!(found.is_some());
        assert!(found.unwrap().parent.is_some());
    }

    #[test]
    fn test_locals() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new();

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals_mut() {
        let ptr = ObjectPointer::null();
        let binding = Binding::new();

        binding.set_local(0, ptr);

        assert_eq!(binding.locals_mut().len(), 1);
    }

    #[test]
    fn test_push_pointers() {
        let local1 = ObjectPointer::new(0x2 as RawObjectPointer);
        let binding1 = Binding::new();

        binding1.set_local(0, local1);

        let local2 = ObjectPointer::new(0x4 as RawObjectPointer);
        let binding2 = Binding::with_parent(binding1.clone());

        binding2.set_local(0, local2);

        let mut pointers = Vec::new();

        binding2.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 2);

        assert!(*pointers[0].get() == local2);
        assert!(*pointers[1].get() == local1);
    }

    #[test]
    fn test_pointers() {
        let b1_local1 = ObjectPointer::new(0x2 as RawObjectPointer);
        let b1_local2 = ObjectPointer::new(0x3 as RawObjectPointer);
        let b1 = Binding::new();

        b1.set_local(0, b1_local1);
        b1.set_local(1, b1_local2);

        let b2_local1 = ObjectPointer::new(0x5 as RawObjectPointer);
        let b2_local2 = ObjectPointer::new(0x6 as RawObjectPointer);
        let b2 = Binding::with_parent(b1.clone());

        b2.set_local(0, b2_local1);
        b2.set_local(1, b2_local2);

        let mut iterator = b2.pointers();

        assert!(iterator.next().unwrap().get() == &b2_local1);
        assert!(iterator.next().unwrap().get() == &b2_local2);

        assert!(iterator.next().unwrap().get() == &b1_local1);
        assert!(iterator.next().unwrap().get() == &b1_local2);

        assert!(iterator.next().is_none());
    }

    #[test]
    fn test_clone_to() {
        let global_alloc = GlobalAllocator::new();
        let mut alloc1 = LocalAllocator::new(global_alloc.clone());
        let mut alloc2 = LocalAllocator::new(global_alloc);

        let ptr1 = alloc1.allocate_without_prototype(object_value::integer(5));
        let ptr2 = alloc1.allocate_without_prototype(object_value::integer(2));

        let src_bind1 = Binding::new();
        let src_bind2 = Binding::with_parent(src_bind1.clone());

        src_bind1.set_local(0, ptr1);
        src_bind2.set_local(0, ptr2);

        let bind_copy = src_bind2.clone_to(&mut alloc2);

        assert_eq!(bind_copy.locals().len(), 1);
        assert!(bind_copy.parent.is_some());

        assert_eq!(bind_copy.get_local(0)
                       .unwrap()
                       .get()
                       .value
                       .as_integer()
                       .unwrap(),
                   2);

        let bind_copy_parent = bind_copy.parent.as_ref().unwrap();

        assert_eq!(bind_copy_parent.locals().len(), 1);
        assert!(bind_copy_parent.parent.is_none());

        assert_eq!(bind_copy_parent.get_local(0)
                       .unwrap()
                       .get()
                       .value
                       .as_integer()
                       .unwrap(),
                   5);
    }
}
