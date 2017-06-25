use deref_pointer::DerefPointer;
use mutability::Mutability;
use types::Type;

/// A single symbol storing information such as the name, the type, mutability,
/// and so on.
#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub value_type: Type,
    pub index: usize,
    pub mutability: Mutability,
}

/// A raw, automatically dereferencing pointer to a symbol.
pub type SymbolPointer = DerefPointer<Symbol>;

impl Symbol {
    /// Returns true if the symbol can be re-defined or if the value can be
    /// mutated.
    pub fn is_mutable(&self) -> bool {
        self.mutability == Mutability::Mutable
    }
}
