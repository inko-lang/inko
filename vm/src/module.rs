//! Modules containing bytecode objects and global variables.

use compiled_code::{CompiledCode, CompiledCodePointer};
use deref_pointer::DerefPointer;
use global_scope::{GlobalScope, GlobalScopePointer};

/// A module is a single file containing bytecode and an associated global
/// scope.
pub struct Module {
    /// The code to execute for this module.
    pub code: Box<CompiledCode>,

    /// The global scope of this module.
    ///
    /// The scope is stored as a Box so that moving around a Module won't
    /// invalidate any GlobalScopePointer instances.
    pub global_scope: Box<GlobalScope>,
}

// A module is immutable once created. The lines below ensure we can store a
// Module in ModuleRegistry without needing a special Sync/Send type (e.g. Arc).
unsafe impl Sync for Module {}
unsafe impl Send for Module {}

impl Module {
    pub fn new(code: CompiledCode) -> Self {
        Module {
            code: Box::new(code),
            global_scope: Box::new(GlobalScope::new()),
        }
    }

    pub fn code(&self) -> CompiledCodePointer {
        DerefPointer::new(&*self.code)
    }

    pub fn global_scope_ref(&self) -> GlobalScopePointer {
        GlobalScopePointer::new(&self.global_scope)
    }
}
