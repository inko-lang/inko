#[derive(Debug)]
pub enum InstructionType {
    SetObject,
    SetInteger,
    SetFloat,
    SetString,
    SetArray,
    SetHash,
    SetLocal,
    GetLocal,
    SetConstant,
    GetConstant,
    SetInstanceVariable,
    GetInstanceVariable,
    Send,
    Return,
    GotoIfUndef,
    GotoIfDef
}

pub struct Instruction {
    pub instruction_type: InstructionType,
    pub arguments: Vec<usize>,
    pub line: usize,
    pub column: usize
}

impl Instruction {
    pub fn new(ins_type: InstructionType, arguments: Vec<usize>, line: usize, column: usize) -> Instruction {
        Instruction {
            instruction_type: ins_type,
            arguments: arguments,
            line: line,
            column: column
        }
    }
}
