//! Executable blocks of code.
//!
//! A Block is an executable block of code (based on a CompiledCode) combined
//! with binding of the scope the block was created in.
use crate::binding::RcBinding;
use crate::compiled_code::CompiledCodePointer;
use crate::deref_pointer::DerefPointer;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;

#[derive(Clone)]
pub struct Block {
    /// The CompiledCode containing the instructions to run.
    pub code: CompiledCodePointer,

    /// The binding this block captures variables from, if any.
    pub captures_from: Option<RcBinding>,

    /// The receiver of the block.
    pub receiver: ObjectPointer,

    /// A pointer to the module this block belongs to.
    ///
    /// Since blocks are created frequently, we don't want the overhead of
    /// atomic reference counting that comes with using an
    /// `ArcWithoutWeak<Module>`. Since modules are not dropped at runtime, we
    /// can safely use a pointer instead.
    pub module: DerefPointer<Module>,
}

impl Block {
    pub fn new(
        code: CompiledCodePointer,
        captures_from: Option<RcBinding>,
        receiver: ObjectPointer,
        module: &Module,
    ) -> Self {
        Block {
            code,
            captures_from,
            receiver,
            module: DerefPointer::new(module),
        }
    }

    pub fn locals(&self) -> u16 {
        self.code.locals
    }
}
