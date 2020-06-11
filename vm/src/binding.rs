//! Variable Bindings
//!
//! A binding contains the local variables available to a certain scope.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::block::Block;
use crate::chunk::Chunk;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use std::cell::UnsafeCell;

pub struct Binding {
    /// The local variables in the current binding.
    ///
    /// Local variables must **not** be modified concurrently as access is not
    /// synchronized due to 99% of all operations being process-local.
    pub locals: UnsafeCell<Chunk<ObjectPointer>>,

    /// The receiver to use when sending messages without an explicit receiver.
    pub receiver: ObjectPointer,

    /// The parent binding, if any.
    pub parent: Option<RcBinding>,
}

pub type RcBinding = ArcWithoutWeak<Binding>;

impl Binding {
    /// Returns a new binding.
    pub fn with_rc(locals: usize, receiver: ObjectPointer) -> RcBinding {
        let bind = Binding {
            locals: UnsafeCell::new(Chunk::new(locals)),
            receiver,
            parent: None,
        };

        ArcWithoutWeak::new(bind)
    }

    #[inline(always)]
    pub fn from_block(block: &Block) -> RcBinding {
        ArcWithoutWeak::new(Binding {
            locals: UnsafeCell::new(Chunk::new(block.locals())),
            receiver: block.receiver,
            parent: block.captures_from.clone(),
        })
    }

    /// Returns the value of a local variable.
    pub fn get_local(&self, index: usize) -> ObjectPointer {
        self.locals()[index]
    }

    /// Sets a local variable.
    pub fn set_local(&mut self, index: usize, value: ObjectPointer) {
        self.locals_mut()[index] = value;
    }

    /// Returns true if the local variable exists.
    pub fn local_exists(&self, index: usize) -> bool {
        !self.get_local(index).is_null()
    }

    /// Returns the parent binding.
    pub fn parent(&self) -> Option<RcBinding> {
        self.parent.clone()
    }

    /// Tries to find a parent binding while limiting the amount of bindings to
    /// traverse.
    pub fn find_parent(&self, depth: usize) -> Option<RcBinding> {
        let mut found = self.parent();

        for _ in 0..depth {
            if let Some(unwrapped) = found {
                found = unwrapped.parent();
            } else {
                return None;
            }
        }

        found
    }

    /// Returns an immutable reference to this binding's local variables.
    pub fn locals(&self) -> &Chunk<ObjectPointer> {
        unsafe { &*self.locals.get() }
    }

    /// Returns a mutable reference to this binding's local variables.
    pub fn locals_mut(&mut self) -> &mut Chunk<ObjectPointer> {
        unsafe { &mut *self.locals.get() }
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        let mut current = Some(self);

        while let Some(binding) = current {
            callback(binding.receiver.pointer());

            for index in 0..binding.locals().len() {
                let local = &binding.locals()[index];

                if !local.is_null() {
                    callback(local.pointer());
                }
            }

            current = binding.parent.as_deref();
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

        let locals = self.locals();
        let mut new_locals = Chunk::new(locals.len());

        for index in 0..locals.len() {
            let pointer = locals[index];

            if !pointer.is_null() {
                new_locals[index] = heap.copy_object(pointer);
            }
        }

        let receiver_copy = heap.copy_object(self.receiver);

        ArcWithoutWeak::new(Binding {
            locals: UnsafeCell::new(new_locals),
            receiver: receiver_copy,
            parent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::immix::local_allocator::LocalAllocator;
    use crate::object_pointer::ObjectPointer;
    use crate::object_value;

    fn binding_with_parent(parent: RcBinding, locals: usize) -> RcBinding {
        let mut binding = Binding::with_rc(locals, ObjectPointer::integer(1));

        binding.parent = Some(parent.clone());

        binding
    }

    #[test]
    fn test_new() {
        let binding = Binding::with_rc(2, ObjectPointer::integer(1));

        assert_eq!(binding.locals().len(), 2);
    }

    #[test]
    fn test_with_parent() {
        let binding1 = Binding::with_rc(0, ObjectPointer::integer(1));
        let binding2 = binding_with_parent(binding1.clone(), 1);

        assert!(binding2.parent.is_some());
        assert_eq!(binding2.locals().len(), 1);
    }

    #[test]
    fn test_get_local_valid() {
        let ptr = ObjectPointer::integer(5);
        let mut binding = Binding::with_rc(1, ObjectPointer::integer(1));

        binding.set_local(0, ptr);

        assert!(binding.get_local(0) == ptr);
    }

    #[test]
    fn test_set_local() {
        let ptr = ObjectPointer::integer(5);
        let mut binding = Binding::with_rc(1, ObjectPointer::integer(1));

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_local_exists_non_existing_local() {
        let binding = Binding::with_rc(1, ObjectPointer::integer(1));

        assert_eq!(binding.local_exists(0), false);
    }

    #[test]
    fn test_local_exists_existing_local() {
        let ptr = ObjectPointer::integer(5);
        let mut binding = Binding::with_rc(1, ObjectPointer::integer(1));

        binding.set_local(0, ptr);

        assert!(binding.local_exists(0));
    }

    #[test]
    fn test_parent_without_parent() {
        let binding = Binding::with_rc(0, ObjectPointer::integer(1));

        assert!(binding.parent().is_none());
    }

    #[test]
    fn test_parent_with_parent() {
        let binding1 = Binding::with_rc(0, ObjectPointer::integer(1));
        let binding2 = binding_with_parent(binding1, 0);

        assert!(binding2.parent().is_some());
    }

    #[test]
    fn test_find_parent_without_parent() {
        let binding = Binding::with_rc(0, ObjectPointer::integer(1));

        assert!(binding.find_parent(0).is_none());
    }

    #[test]
    fn test_find_parent_with_parent() {
        let binding1 = Binding::with_rc(0, ObjectPointer::integer(1));
        let binding2 = binding_with_parent(binding1, 0);
        let binding3 = binding_with_parent(binding2, 0);
        let binding4 = binding_with_parent(binding3, 0);

        assert!(binding4.find_parent(0).is_some());
        assert!(binding4.find_parent(0).unwrap().parent().is_some());

        assert!(binding4.find_parent(1).is_some());
        assert!(binding4.find_parent(1).unwrap().parent().is_some());

        assert!(binding4.find_parent(2).is_some());
        assert!(binding4.find_parent(2).unwrap().parent().is_none());

        assert!(binding4.find_parent(3).is_none());
    }

    #[test]
    fn test_locals() {
        let ptr = ObjectPointer::integer(5);
        let mut binding = Binding::with_rc(1, ObjectPointer::integer(1));

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals_mut() {
        let ptr = ObjectPointer::integer(5);
        let mut binding = Binding::with_rc(1, ObjectPointer::integer(1));

        binding.set_local(0, ptr);

        assert_eq!(binding.locals_mut().len(), 1);
    }

    #[test]
    fn test_each_pointer() {
        let mut alloc =
            LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new());

        let local1 = alloc.allocate_empty();
        let receiver = alloc.allocate_empty();
        let mut binding1 = Binding::with_rc(1, receiver);

        binding1.set_local(0, local1);

        let local2 = alloc.allocate_empty();
        let mut binding2 = Binding::with_rc(1, receiver);

        binding2.parent = Some(binding1.clone());
        binding2.set_local(0, local2);

        let mut pointer_pointers = Vec::new();

        binding2.each_pointer(|ptr| pointer_pointers.push(ptr));

        let pointers: Vec<_> =
            pointer_pointers.into_iter().map(|x| *x.get()).collect();

        assert_eq!(pointers.iter().filter(|x| **x == receiver).count(), 2);
        assert!(pointers.contains(&local2));
        assert!(pointers.contains(&local1));
    }

    #[test]
    fn test_each_pointer_and_update() {
        let mut alloc =
            LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new());

        let mut binding = Binding::with_rc(1, alloc.allocate_empty());
        let mut pointers = Vec::new();

        binding.set_local(0, alloc.allocate_empty());

        binding.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            let pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as _;
        }

        assert_eq!(binding.get_local(0).raw.raw as usize, 0x4);
        assert_eq!(binding.receiver.raw.raw as usize, 0x4);
    }

    #[test]
    fn test_clone_to() {
        let global_alloc = GlobalAllocator::with_rc();
        let mut alloc1 =
            LocalAllocator::new(global_alloc.clone(), &Config::new());
        let mut alloc2 = LocalAllocator::new(global_alloc, &Config::new());

        let ptr1 = alloc1.allocate_without_prototype(object_value::float(5.0));
        let ptr2 = alloc1.allocate_without_prototype(object_value::float(2.0));
        let ptr3 = alloc1.allocate_without_prototype(object_value::float(8.0));

        let mut src_bind1 = Binding::with_rc(1, ptr3);
        let mut src_bind2 = Binding::with_rc(1, ptr3);

        src_bind2.parent = Some(src_bind1.clone());
        src_bind1.set_local(0, ptr1);
        src_bind2.set_local(0, ptr2);

        let bind_copy = src_bind2.clone_to(&mut alloc2);

        assert_eq!(bind_copy.locals().len(), 1);
        assert!(bind_copy.parent.is_some());

        assert_eq!(bind_copy.get_local(0).float_value().unwrap(), 2.0);

        let bind_copy_parent = bind_copy.parent.as_ref().unwrap();

        assert_eq!(bind_copy_parent.locals().len(), 1);
        assert!(bind_copy_parent.parent.is_none());

        assert_eq!(bind_copy_parent.get_local(0).float_value().unwrap(), 5.0);
        assert_eq!(bind_copy_parent.receiver.float_value().unwrap(), 8.0);
    }

    #[test]
    fn test_receiver_with_receiver() {
        let pointer = ObjectPointer::integer(5);
        let binding = Binding::with_rc(1, pointer);

        assert!(binding.receiver == pointer);
    }
}
