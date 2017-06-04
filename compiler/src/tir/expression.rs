use tir::code_object::CodeObject;
use tir::implement::Implement;
use tir::import::Symbol as ImportSymbol;
use tir::method::{MethodArgument, MethodType};
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

    Closure {
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
        name: String,
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    GetAttribute {
        receiver: Box<Expression>,
        name: String,
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

    Class {
        name: String,
        implements: Vec<Implement>,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    Trait {
        name: String,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    Method {
        receiver: Box<Expression>,
        name: String,
        method_type: MethodType,
        arguments: Vec<MethodArgument>,
        requires: Vec<Expression>,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    RequiredMethod {
        name: String,
        arguments: Vec<MethodArgument>,
        requires: Vec<Expression>,
        line: usize,
        column: usize,
    },

    GetSelf { line: usize, column: usize },

    ImportModule {
        path: String,
        line: usize,
        column: usize,
        symbols: Vec<ImportSymbol>,
    },

    Return {
        value: Box<Expression>,
        line: usize,
        column: usize,
    },

    Nil { line: usize, column: usize },

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
}
