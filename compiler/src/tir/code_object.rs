use tir::expression::Expression;
use tir::variable::Scope as VariableScope;

#[derive(Debug, Clone)]
pub struct CodeObject {
    pub locals: VariableScope,
    pub body: Vec<Expression>,
}
