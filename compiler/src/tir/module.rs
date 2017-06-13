//! Modules containing source code.
use tir::expression::Expression;
use tir::variable::Scope as VariableScope;

#[derive(Debug)]
pub struct Module {
    /// The file path of this module.
    pub path: String,

    /// The name of this module.
    pub name: String,

    /// The body of the module.
    pub body: Expression,

    /// The global variables defined in this module.
    pub globals: VariableScope,
}
