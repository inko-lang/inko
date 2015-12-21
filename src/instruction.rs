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
    GetIntegerPrototype,
    GetFloatPrototype,
    GetStringPrototype,
    GetArrayPrototype,
    GetThreadPrototype,
    GetTruePrototype,
    GetFalsePrototype,
    GetMethodPrototype,
    SetTrue,
    SetFalse,
    SetLocal,
    GetLocal,
    SetConst,
    GetConst,
    SetAttr,
    GetAttr,
    Send,
    Return,
    GotoIfFalse,
    GotoIfTrue,
    Goto,
    DefMethod,
    DefLiteralMethod,
    RunCode,
    GetToplevel,
    IsError,
    ErrorToString,
    IntegerAdd,
    IntegerDiv,
    IntegerMul,
    IntegerSub,
    IntegerMod,
    IntegerToFloat,
    IntegerToString,
    IntegerBitwiseAnd,
    IntegerBitwiseOr,
    IntegerBitwiseXor,
    IntegerShiftLeft,
    IntegerShiftRight,
    IntegerSmaller,
    IntegerGreater,
    IntegerEquals,
    StartThread,
    FloatAdd,
    FloatMul,
    FloatDiv,
    FloatSub,
    FloatMod,
    FloatToInteger,
    FloatToString,
    FloatSmaller,
    FloatGreater,
    FloatEquals,
    ArrayInsert,
    ArrayAt,
    ArrayRemove,
    ArrayLength,
    ArrayClear,
    StringToLower,
    StringToUpper,
    StringEquals,
    StringToBytes,
    StringFromBytes,
    StringLength,
    StringSize,
    StdoutWrite,
    StderrWrite,
    StdinRead,
    StdinReadLine,
    FileOpen,
    FileWrite,
    FileRead,
    FileReadLine,
    FileFlush,
    FileSize,
    FileSeek
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

    pub fn arg(&self, index: usize) -> Result<usize, String> {
        self.arguments
            .get(index)
            .cloned()
            .ok_or(format!("undefined instruction argument {}", index))
    }
}
