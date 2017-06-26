use rc_cell::RcCell;
use symbol::RcSymbol;
use symbol_table::SymbolTable;
use types::Type;
use types::object::Object;

#[derive(Debug)]
pub struct Block {
    /// The name of the block, if any.
    pub name: Option<String>,

    /// The local variables defined in this block along with their type
    /// information.
    pub locals_table: SymbolTable,

    /// The arguments of the block. The symbols are defined in the locals symbol
    /// table.
    pub arguments: Vec<RcSymbol>,

    /// A symbol table used for storing the type arguments this block may take.
    pub type_arguments: SymbolTable,

    /// The type of the value this block may throw.
    pub throw_type: Option<Type>,

    /// The type of the value this block will return.
    pub return_type: Type,

    /// The attributes defined directly on this block.
    pub attributes: SymbolTable,

    /// The methods defined directly on this block.
    pub methods: SymbolTable,

    /// The prototype of this block.
    pub prototype: RcCell<Object>,
}
