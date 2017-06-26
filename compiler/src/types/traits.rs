use rc_cell::RcCell;
use symbol_table::SymbolTable;
use types::object::Object;

#[derive(Debug)]
pub struct Trait {
    /// The name of the trait.
    pub name: String,

    /// The methods defined directly on the trait.
    pub methods: SymbolTable,

    /// The attributes defined directly on the trait.
    pub attributes: SymbolTable,

    /// The instance methods of the trait.
    pub instance_methods: SymbolTable,

    /// The type arguments this trait may have.
    pub type_arguments: SymbolTable,

    /// The prototype of this trait.
    pub prototype: RcCell<Object>,
}
