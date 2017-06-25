//! Modules containing source code.
use symbol_table::SymbolTable;
use tir::expression::Expression;

#[derive(Debug)]
pub struct Module {
    /// The file path of this module.
    pub path: String,

    /// The name of this module.
    pub name: String,

    /// The body of the module.
    pub body: Expression,

    /// The global variables defined in this module.
    pub globals: SymbolTable,
}
