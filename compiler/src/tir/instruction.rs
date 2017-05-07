use tir::code_object::CodeObject;
use tir::method::{MethodArgument, MethodRequirement};
use tir::registers::Register;
use tir::variable::Variable;

#[derive(Debug)]
pub enum Instruction {
    SetInteger {
        register: Register,
        value: i64,
        line: usize,
        column: usize,
    },

    SetFloat {
        register: Register,
        value: f64,
        line: usize,
        column: usize,
    },

    SetString {
        register: Register,
        value: String,
        line: usize,
        column: usize,
    },

    SetArray {
        register: Register,
        values: Vec<Register>,
        line: usize,
        column: usize,
    },

    SetHash {
        register: Register,
        pairs: Vec<(Register, Register)>,
        line: usize,
        column: usize,
    },

    GetConstant {
        register: Register,
        receiver: Register,
        name: String,
        line: usize,
        column: usize,
    },

    SetConstant {
        register: Register,
        receiver: Register,
        name: String,
        value: Register,
        line: usize,
        column: usize,
    },

    GetLocal {
        register: Register,
        variable: Variable,
        line: usize,
        column: usize,
    },

    SetLocal {
        register: Register,
        variable: Variable,
        value: Register,
        line: usize,
        column: usize,
    },

    GetGlobal {
        register: Register,
        variable: Variable,
        line: usize,
        column: usize,
    },

    SetGlobal {
        register: Register,
        variable: Variable,
        value: Register,
        line: usize,
        column: usize,
    },

    SetAttribute {
        register: Register,
        receiver: Register,
        name: String,
        value: Register,
        line: usize,
        column: usize,
    },

    GetAttribute {
        register: Register,
        receiver: Register,
        name: String,
        line: usize,
        column: usize,
    },

    SendObjectMessage {
        register: Register,
        receiver: Register,
        name: String,
        arguments: Vec<Register>,
        line: usize,
        column: usize,
    },

    DefClass {
        register: Register,
        receiver: Register,
        name: String,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    DefTrait {
        register: Register,
        receiver: Register,
        name: String,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    DefMethod {
        register: Register,
        receiver: Register,
        name: String,
        arguments: Vec<MethodArgument>,
        requires: Vec<MethodRequirement>,
        body: CodeObject,
        line: usize,
        column: usize,
    },

    GetSelf {
        register: Register,
        line: usize,
        column: usize,
    },
}
