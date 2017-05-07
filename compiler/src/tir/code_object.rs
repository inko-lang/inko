use tir::instruction::Instruction;
use tir::registers::Registers;
use tir::variable::Scope as VariableScope;

#[derive(Debug)]
pub struct CodeObject {
    pub registers: Registers,
    pub variables: VariableScope,
    pub instructions: Vec<Instruction>,
}

impl CodeObject {
    pub fn new() -> Self {
        CodeObject {
            registers: Registers::new(),
            variables: VariableScope::new(),
            instructions: Vec::new(),
        }
    }
}
