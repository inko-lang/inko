use symbol_table::SymbolTable;
use tir::expression::Expression;

#[derive(Debug)]
pub struct CodeObject {
    pub locals: SymbolTable,
    pub body: Vec<Expression>,
}
