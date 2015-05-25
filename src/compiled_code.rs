use instruction::Instruction;

pub struct CompiledCode<'l> {
    pub name: &'l str,
    pub file: &'l str,
    pub line: usize,
    pub required_arguments: usize,
    pub optional_arguments: usize,
    pub rest_argument: bool,
    pub locals: Vec<&'l str>, // THINK: use Vec<String> instead?
    pub instructions: Vec<Instruction>
}
