//! Executable blocks of code.
//!
//! A Block is an executable block of code (based on a CompiledCode) combined
//! with binding of the scope the block was created in.
use binding::RcBinding;
use compiled_code::CompiledCodePointer;
use global_scope::GlobalScopePointer;

#[derive(Clone)]
pub struct Block {
    /// The CompiledCode containing the instructions to run.
    pub code: CompiledCodePointer,

    /// The binding of the scope in which this block was created.
    pub binding: RcBinding,

    /// The global scope this block belongs to.
    pub global_scope: GlobalScopePointer,
}

impl Block {
    pub fn new(
        code: CompiledCodePointer,
        binding: RcBinding,
        global_scope: GlobalScopePointer,
    ) -> Self {
        Block {
            code,
            binding,
            global_scope,
        }
    }

    pub fn locals(&self) -> usize {
        self.code.locals as usize
    }
}
