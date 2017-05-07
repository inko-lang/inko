use tir::instruction::Instruction;

#[derive(Debug)]
pub struct MethodRequirement {
    pub instructions: Vec<Instruction>,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub struct MethodArgument {
    pub name: String,
    pub default_value: Option<Vec<Instruction>>,
    pub line: usize,
    pub column: usize,
    pub rest: bool,
}
