use symbol::RcSymbol;
use tir::code_object::CodeObject;
use tir::import::Symbol as ImportSymbol;
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
    },

    String {
        value: String,
        line: usize,
        column: usize,
    },

    Array {
        values: Vec<Expression>,
        line: usize,
        column: usize,
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
    },

    SetGlobal {
        variable: RcSymbol,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    SetAttribute {
        receiver: Box<Expression>,
        name: Box<Expression>,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    GetAttribute {
        receiver: Box<Expression>,
        name: Box<Expression>,
        line: usize,
        column: usize,
    },

    SendObjectMessage {
        receiver: Box<Expression>,
        name: String,
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

    KeywordArgument {
        name: String,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    ImportModule {
        path: Box<Expression>,
        symbols: Vec<ImportSymbol>,
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

    DefineMethod {
        receiver: Box<Expression>,
        name: Box<Expression>,
        block: Box<Expression>,
        line: usize,
        column: usize,
    },

    DefineRequiredMethod {
        receiver: Box<Expression>,
        name: Box<Expression>,
        line: usize,
        column: usize,
    },

    DefineClass {
        receiver: Box<Expression>,
        name: Box<Expression>,
        body: Box<Expression>,
        line: usize,
        column: usize,
    },

    DefineTrait {
        receiver: Box<Expression>,
        name: Box<Expression>,
        body: Box<Expression>,
        line: usize,
        column: usize,
    },

    DefineModule {
        name: Box<Expression>,
        body: CodeObject,
        line: usize,
        column: usize,
    },
}

impl Expression {
    /// Returns the type of the expression.
    ///
    /// Since "type" is a keyword this function is called "kind" instead.
    pub fn kind<'a>(&self) -> Type {
        match self {
            &Expression::GetBlockPrototype { ref kind, .. } |
            &Expression::GetIntegerPrototype { ref kind, .. } |
            &Expression::Integer { ref kind, .. } |
            &Expression::Block { ref kind, .. } => kind.clone(),
            _ => Type::Dynamic,
        }
    }
}
