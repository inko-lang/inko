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
            code: code,
            binding: binding,
            global_scope: global_scope,
        }
    }

    #[inline(always)]
    pub fn arguments(&self) -> usize {
        self.code.arguments as usize
    }

    #[inline(always)]
    pub fn required_arguments(&self) -> usize {
        self.code.required_arguments as usize
    }

    pub fn locals(&self) -> usize {
        self.code.locals as usize
    }

    pub fn has_rest_argument(&self) -> bool {
        self.code.rest_argument
    }

    pub fn name(&self) -> &String {
        &self.code.name
    }

    pub fn label_for_number_of_arguments(&self) -> String {
        if self.has_rest_argument() {
            format!("{}+", self.arguments())
        } else {
            format!("{}", self.arguments())
        }
    }

    pub fn valid_number_of_arguments(&self, given: usize) -> bool {
        let total = self.arguments();
        let required = self.required_arguments();

        if given < required {
            return false;
        }

        if given > total && !self.has_rest_argument() {
            return false;
        }

        true
    }
}
