use symbol::RcSymbol;
use tir::code_object::CodeObject;
use types::Type;

#[derive(Debug)]
pub struct Argument {
    pub name: String,
    pub default_value: Option<Expression>,
    pub line: usize,
    pub column: usize,
    pub rest: bool,
}

#[derive(Debug)]
pub enum Expression {
    Void,

    Expressions { nodes: Vec<Expression> },

    Integer {
        value: i64,
        line: usize,
        column: usize,
        kind: Type,
    },

    Float {
        value: f64,
        line: usize,
        column: usize,
        kind: Type,
    },

    String {
        value: String,
        line: usize,
        column: usize,
        kind: Type,
    },

    Array {
        values: Vec<Expression>,
        line: usize,
        column: usize,
        kind: Type,
    },

    Hash {
        pairs: Vec<(Expression, Expression)>,
        line: usize,
        column: usize,
    },

    Block {
        arguments: Vec<Argument>,
        body: CodeObject,
        line: usize,
        column: usize,
        kind: Type,
    },

    GetLocal {
        variable: RcSymbol,
        line: usize,
        column: usize,
        kind: Type,
    },

    SetLocal {
        variable: RcSymbol,
        value: Box<Expression>,
        line: usize,
        column: usize,
        kind: Type,
    },

    GetGlobal {
        variable: RcSymbol,
        line: usize,
        column: usize,
        kind: Type,
    },

    SetGlobal {
        variable: RcSymbol,
        value: Box<Expression>,
        line: usize,
        column: usize,
        kind: Type,
    },

    SetAttribute {
        receiver: Box<Expression>,
        name: Box<Expression>,
        value: Box<Expression>,
        line: usize,
        column: usize,
        kind: Type,
    },

    GetAttribute {
        receiver: Box<Expression>,
        name: Box<Expression>,
        line: usize,
        column: usize,
    },

    SendObjectMessage {
        receiver: Box<Expression>,
        name: Box<Expression>,
        arguments: Vec<Expression>,
        line: usize,
        column: usize,
    },

    GetBlockPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetIntegerPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetFloatPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetStringPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetArrayPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetBooleanPrototype {
        line: usize,
        column: usize,
        kind: Type,
    },

    SetObject {
        arguments: Vec<Expression>,
        line: usize,
        column: usize,
        kind: Type,
    },

    KeywordArgument {
        name: String,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    Return {
        value: Option<Box<Expression>>,
        line: usize,
        column: usize,
    },

    Try {
        body: CodeObject,
        else_body: Option<CodeObject>,
        else_argument: Option<RcSymbol>,
        line: usize,
        column: usize,
    },

    Throw {
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    DefineModule {
        name: Box<Expression>,
        body: CodeObject,
        line: usize,
        column: usize,
        kind: Type,
    },

    GetTopLevel {
        line: usize,
        column: usize,
        kind: Type,
    },

    GetTemporary {
        id: usize,
        line: usize,
        column: usize,
    },

    SetTemporary {
        id: usize,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },
}

impl Expression {
    /// Returns the type of the expression.
    ///
    /// Since "type" is a keyword this function is called "kind" instead.
    pub fn kind(&self) -> Type {
        match self {
            &Expression::Integer { ref kind, .. } |
            &Expression::Float { ref kind, .. } |
            &Expression::String { ref kind, .. } |
            &Expression::Array { ref kind, .. } |
            &Expression::Block { ref kind, .. } |
            &Expression::GetLocal { ref kind, .. } |
            &Expression::SetLocal { ref kind, .. } |
            &Expression::GetGlobal { ref kind, .. } |
            &Expression::SetGlobal { ref kind, .. } |
            &Expression::SetAttribute { ref kind, .. } |
            &Expression::GetBlockPrototype { ref kind, .. } |
            &Expression::GetIntegerPrototype { ref kind, .. } |
            &Expression::GetFloatPrototype { ref kind, .. } |
            &Expression::GetStringPrototype { ref kind, .. } |
            &Expression::GetArrayPrototype { ref kind, .. } |
            &Expression::GetBooleanPrototype { ref kind, .. } |
            &Expression::SetObject { ref kind, .. } |
            &Expression::GetTopLevel { ref kind, .. } |
            &Expression::DefineModule { ref kind, .. } => kind.clone(),
            _ => Type::Dynamic,
        }
    }
}
