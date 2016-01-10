//! Virtual Machine Instructions
//!
//! An Instruction contains information about a single instruction such as the
//! type and arguments.

/// Enum containing all possible instruction types.
#[derive(Debug, Clone)]
#[repr(u16)]
pub enum InstructionType {
    SetInteger               = 0,
    SetFloat                 = 1,
    SetString                = 2,
    SetObject                = 3,
    SetArray                 = 4,
    SetName                  = 5,
    GetIntegerPrototype      = 6,
    GetFloatPrototype        = 7,
    GetStringPrototype       = 8,
    GetArrayPrototype        = 9,
    GetThreadPrototype       = 10,
    GetTruePrototype         = 11,
    GetFalsePrototype        = 12,
    GetMethodPrototype       = 13,
    GetCompiledCodePrototype = 14,
    SetTrue                  = 15,
    SetFalse                 = 16,
    SetLocal                 = 17,
    GetLocal                 = 18,
    SetConst                 = 19,
    GetConst                 = 20,
    SetAttr                  = 21,
    GetAttr                  = 22,
    SetCompiledCode          = 23,
    Send                     = 24,
    Return                   = 25,
    GotoIfFalse              = 26,
    GotoIfTrue               = 27,
    Goto                     = 28,
    DefMethod                = 29,
    DefLiteralMethod         = 30,
    RunCode                  = 31,
    GetToplevel              = 32,
    IsError                  = 33,
    ErrorToString            = 34,
    IntegerAdd               = 35,
    IntegerDiv               = 36,
    IntegerMul               = 37,
    IntegerSub               = 38,
    IntegerMod               = 39,
    IntegerToFloat           = 40,
    IntegerToString          = 41,
    IntegerBitwiseAnd        = 42,
    IntegerBitwiseOr         = 43,
    IntegerBitwiseXor        = 44,
    IntegerShiftLeft         = 45,
    IntegerShiftRight        = 46,
    IntegerSmaller           = 47,
    IntegerGreater           = 48,
    IntegerEquals            = 49,
    StartThread              = 50,
    FloatAdd                 = 51,
    FloatMul                 = 52,
    FloatDiv                 = 53,
    FloatSub                 = 54,
    FloatMod                 = 55,
    FloatToInteger           = 56,
    FloatToString            = 57,
    FloatSmaller             = 58,
    FloatGreater             = 59,
    FloatEquals              = 60,
    ArrayInsert              = 61,
    ArrayAt                  = 62,
    ArrayRemove              = 63,
    ArrayLength              = 64,
    ArrayClear               = 65,
    StringToLower            = 66,
    StringToUpper            = 67,
    StringEquals             = 68,
    StringToBytes            = 69,
    StringFromBytes          = 70,
    StringLength             = 71,
    StringSize               = 72,
    StdoutWrite              = 73,
    StderrWrite              = 74,
    StdinRead                = 75,
    StdinReadLine            = 76,
    FileOpen                 = 77,
    FileWrite                = 78,
    FileRead                 = 79,
    FileReadLine             = 80,
    FileFlush                = 81,
    FileSize                 = 82,
    FileSeek                 = 83,
    RunFileFast              = 84
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
