use symbol_table::SymbolTable;
use tir::expression::Expression;

#[derive(Debug)]
pub struct CodeObject {
    pub locals: SymbolTable,
    pub body: Expression,
}

impl CodeObject {
    pub fn new(locals: SymbolTable, body: Expression) -> Self {
        CodeObject { locals: locals, body: body }
    }
}
