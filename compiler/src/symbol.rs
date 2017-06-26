use rc_cell::RcCell;
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

/// A reference counted symbol.
pub type RcSymbol = RcCell<Symbol>;

impl Symbol {
    /// Returns true if the symbol can be re-defined or if the value can be
    /// mutated.
    pub fn is_mutable(&self) -> bool {
        self.mutability == Mutability::Mutable
    }
}
