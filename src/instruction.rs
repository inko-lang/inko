//! Virtual Machine Instructions
//!
//! An Instruction contains information about a single instruction such as the
//! type and arguments.

/// Enum containing all possible instruction types.
#[derive(Debug, Clone)]
pub enum InstructionType {
    SetInteger,
    SetFloat,
    SetString,
    SetObject,
    SetArray,
    SetName,
    SetIntegerPrototype,
    SetFloatPrototype,
    SetStringPrototype,
    SetArrayPrototype,
    SetThreadPrototype,
    SetLocal,
    GetLocal,
    SetConst,
    GetConst,
    SetAttr,
    GetAttr,
    Send,
    Return,
    GotoIfUndef,
    GotoIfDef,
    Goto,
    DefMethod,
    RunCode,
    GetToplevel,
    IntegerAdd,
    StartThread
}

/// Struct for storing information about a single instruction.
#[derive(Clone)]
pub struct Instruction {
    /// The type of instruction.
    pub instruction_type: InstructionType,

    /// The arguments of the instruction.
    pub arguments: Vec<usize>,

    /// The line from which the instruction originated.
    pub line: usize,

    /// The column from which the instruction originated.
    pub column: usize
}

impl Instruction {
    /// Returns a new Instruction.
    pub fn new(ins_type: InstructionType, arguments: Vec<usize>, line: usize,
               column: usize) -> Instruction {
        Instruction {
            instruction_type: ins_type,
            arguments: arguments,
            line: line,
            column: column
        }
    }
}
