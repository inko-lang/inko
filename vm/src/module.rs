//! Modules containing bytecode objects and global variables.
use crate::compiled_code::{CompiledCode, CompiledCodePointer};
use crate::deref_pointer::DerefPointer;
use crate::global_scope::GlobalScope;
use crate::object_pointer::ObjectPointer;
use std::sync::atomic::{AtomicBool, Ordering};

/// A module is a single file containing bytecode and an associated global
/// scope.
pub struct Module {
    /// The name of the module, as a String object.
    name: ObjectPointer,

    /// The code to execute for this module.
    code: Box<CompiledCode>,

    /// The global scope of this module.
    ///
    /// The scope is stored as a Box so that moving around a Module won't
    /// invalidate any GlobalScopePointer instances.
    global_scope: Box<GlobalScope>,

    /// The literals (e.g. string literals) stored in this module. All these
    /// literals must be permanent objects.
    literals: Vec<ObjectPointer>,

    /// A boolean indicating if this module has been executed or not.
    ///
    /// This value is atomic so it can be modified through a
    /// ArcWithoutWeak<Module>, even though this value is not modified
    /// concurrently.
    executed: AtomicBool,
}

// A module is immutable once created. The lines below ensure we can store a
// Module in ModuleRegistry without needing a special Sync/Send type (e.g. Arc).
unsafe impl Sync for Module {}
unsafe impl Send for Module {}

impl Module {
    pub fn new(
        name: ObjectPointer,
        code: CompiledCode,
        literals: Vec<ObjectPointer>,
    ) -> Self {
        Module {
            name,
            code: Box::new(code),
            global_scope: Box::new(GlobalScope::new()),
            literals,
            executed: AtomicBool::new(false),
        }
    }

    pub fn name(&self) -> ObjectPointer {
        self.name
    }

    pub fn source_path(&self) -> ObjectPointer {
        self.code.file
    }

    pub fn code(&self) -> CompiledCodePointer {
        DerefPointer::new(&*self.code)
    }

    pub fn global_scope(&self) -> &GlobalScope {
        &self.global_scope
    }

    pub fn global_scope_mut(&mut self) -> &mut GlobalScope {
        &mut self.global_scope
    }

    pub fn mark_as_executed(&self) -> bool {
        if self.executed.load(Ordering::Acquire) {
            return false;
        }

        self.executed.store(true, Ordering::Release);
        true
    }

    #[inline(always)]
    pub unsafe fn literal(&self, index: usize) -> ObjectPointer {
        *self.literals.get_unchecked(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_as_executed() {
        let name = ObjectPointer::null();
        let path = ObjectPointer::null();
        let code = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, code, Vec::new());

        assert_eq!(module.mark_as_executed(), true);
        assert_eq!(module.mark_as_executed(), false);
    }
}
