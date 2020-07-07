//! Executable blocks of code.
//!
//! A Block is an executable block of code (based on a CompiledCode) combined
//! with binding of the scope the block was created in.
use crate::binding::RcBinding;
use crate::compiled_code::CompiledCodePointer;
use crate::global_scope::GlobalScopePointer;
use crate::object_pointer::ObjectPointer;

#[derive(Clone)]
pub struct Block {
    /// The CompiledCode containing the instructions to run.
    pub code: CompiledCodePointer,

    /// The binding this block captures variables from, if any.
    pub captures_from: Option<RcBinding>,

    /// The receiver of the block.
    pub receiver: ObjectPointer,

    /// The global scope this block belongs to.
    pub global_scope: GlobalScopePointer,
}

impl Block {
    pub fn new(
        code: CompiledCodePointer,
        captures_from: Option<RcBinding>,
        receiver: ObjectPointer,
        global_scope: GlobalScopePointer,
    ) -> Self {
        Block {
            code,
            captures_from,
            receiver,
            global_scope,
        }
    }

    pub fn locals(&self) -> u16 {
        self.code.locals
    }
}
