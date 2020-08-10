//! Modules containing bytecode objects and global variables.
use crate::compiled_code::{CompiledCode, CompiledCodePointer};
use crate::deref_pointer::DerefPointer;
use crate::global_scope::GlobalScope;
use crate::object_pointer::ObjectPointer;

/// A module is a single file containing bytecode and an associated global
/// scope.
pub struct Module {
    /// The name of the module, as a String object.
    name: ObjectPointer,

    /// The full path to the bytecode file, as a String object.
    path: ObjectPointer,

    /// The code to execute for this module.
    code: Box<CompiledCode>,

    /// The global scope of this module.
    ///
    /// The scope is stored as a Box so that moving around a Module won't
    /// invalidate any GlobalScopePointer instances.
    global_scope: Box<GlobalScope>,
}

// A module is immutable once created. The lines below ensure we can store a
// Module in ModuleRegistry without needing a special Sync/Send type (e.g. Arc).
unsafe impl Sync for Module {}
unsafe impl Send for Module {}

impl Module {
    pub fn new(
        name: ObjectPointer,
        path: ObjectPointer,
        code: CompiledCode,
    ) -> Self {
        Module {
            name,
            path,
            code: Box::new(code),
            global_scope: Box::new(GlobalScope::new()),
        }
    }

    pub fn name(&self) -> ObjectPointer {
        self.name
    }

    pub fn path(&self) -> ObjectPointer {
        self.path
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
}
