use rc_cell::RcCell;
use symbol::RcSymbol;
use symbol_table::SymbolTable;

#[derive(Debug)]
pub struct Object {
    /// The name of the object, if any.
    pub name: Option<String>,

    /// The attributes defined on this object.
    pub attributes: SymbolTable,

    /// The methods defined on this object.
    pub methods: SymbolTable,

    /// The traits this object implements.
    pub implemented_traits: Vec<RcSymbol>,

    /// The type arguments this object may have.
    pub type_arguments: SymbolTable,

    /// The prototype of this object.
    pub prototype: Option<RcCell<Object>>,
}

impl Object {
    pub fn new() -> Self {
        Object {
            name: None,
            attributes: SymbolTable::new(),
            methods: SymbolTable::new(),
            implemented_traits: Vec::new(),
            type_arguments: SymbolTable::new(),
            prototype: None,
        }
    }

    pub fn with_name(name: &str) -> Self {
        Object {
            name: Some(name.to_string()),
            attributes: SymbolTable::new(),
            methods: SymbolTable::new(),
            implemented_traits: Vec::new(),
            type_arguments: SymbolTable::new(),
            prototype: None,
        }
    }
}
