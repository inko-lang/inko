use tir::expression::Expression;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodType {
    Method,
    InstanceMethod,
}

#[derive(Debug, Clone)]
pub struct MethodArgument {
    pub name: String,
    pub default_value: Option<Expression>,
    pub line: usize,
    pub column: usize,
    pub rest: bool,
}
