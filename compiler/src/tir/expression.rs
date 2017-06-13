use tir::code_object::CodeObject;
use tir::import::Symbol as ImportSymbol;
use tir::method::MethodArgument;
use tir::variable::Variable;

#[derive(Debug, Clone)]
pub enum Expression {
    Void,

    Integer {
        value: i64,
        line: usize,
        column: usize,
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
        arguments: Vec<MethodArgument>,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    GetLocal {
        variable: Variable,
        line: usize,
        column: usize,
    },

    SetLocal {
        variable: Variable,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    GetGlobal {
        variable: Variable,
        line: usize,
        column: usize,
    },

    SetGlobal {
        variable: Variable,
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

    RawInstruction {
        name: String,
        arguments: Vec<Expression>,
        line: usize,
        column: usize,
    },

    KeywordArgument {
        name: String,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    ImportModule {
        path: String,
        line: usize,
        column: usize,
        symbols: Vec<ImportSymbol>,
    },

    Return {
        value: Option<Box<Expression>>,
        line: usize,
        column: usize,
    },

    Try {
        body: CodeObject,
        else_body: Option<CodeObject>,
        else_argument: Option<Variable>,
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
}
