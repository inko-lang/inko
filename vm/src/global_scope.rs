//! Scopes for module-local global variables.
use crate::deref_pointer::DerefPointer;
use crate::object_pointer::ObjectPointer;
use std::cell::UnsafeCell;

/// A GlobalScope contains all the global variables defined in a module.
///
/// Access to variables is _not_ synchronized to reduce overhead. As such one
/// must take care not to modify the list of variables in a concurrent manner.
///
/// Since modules are only executed once this should typically not be a problem.
///
/// Furthermore, a global scope may only contain permanent pointers. This is
/// necessary as otherwise a scope may outlive the variables stored in in.
pub struct GlobalScope {
    variables: UnsafeCell<Vec<ObjectPointer>>,
}

pub type GlobalScopePointer = DerefPointer<GlobalScope>;

impl GlobalScope {
    pub fn new() -> GlobalScope {
        GlobalScope {
            variables: UnsafeCell::new(vec![ObjectPointer::null(); 32]),
        }
    }

    /// Returns a global variable.
    ///
    /// This method will panic when attempting to retrieve a non-existing global
    /// variable.
    pub fn get(&self, index: usize) -> ObjectPointer {
        self.locals()[index]
    }

    /// Sets a global variable.
    pub fn set(&mut self, index: usize, value: ObjectPointer) {
        if !value.is_permanent() {
            panic!("Only permanent objects can be stored in a global scope");
        }

        let locals = self.locals_mut();

        if index >= locals.len() {
            locals.resize(index + 1, ObjectPointer::null());
        }

        locals[index] = value;
    }

    fn locals(&self) -> &Vec<ObjectPointer> {
        unsafe { &*self.variables.get() }
    }

    fn locals_mut(&mut self) -> &mut Vec<ObjectPointer> {
        unsafe { &mut *self.variables.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::immix::local_allocator::LocalAllocator;
    use crate::object_pointer::ObjectPointer;

    mod global_scope {
        use super::*;

        #[test]
        #[should_panic]
        fn test_get_invalid() {
            GlobalScope::new().get(35);
        }

        #[test]
        #[should_panic]
        fn test_set_not_permanent() {
            let mut scope = GlobalScope::new();
            let mut alloc =
                LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new());
            let pointer = alloc.allocate_empty();

            scope.set(0, pointer);
        }

        #[test]
        fn test_get_set() {
            let mut scope = GlobalScope::new();

            scope.set(0, ObjectPointer::integer(5));

            assert!(scope.get(0) == ObjectPointer::integer(5));
        }
    }
}
