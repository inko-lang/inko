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
    GetIntegerPrototype      = 6,
    GetFloatPrototype        = 7,
    GetStringPrototype       = 8,
    GetArrayPrototype        = 9,
    GetThreadPrototype       = 10,
    GetTruePrototype         = 11,
    GetFalsePrototype        = 12,
    GetMethodPrototype       = 13,
    GetCompiledCodePrototype = 14,
    GetTrue                  = 15,
    GetFalse                 = 16,
    SetLocal                 = 17,
    GetLocal                 = 18,
    SetLiteralConst          = 19,
    GetLiteralConst          = 20,
    SetLiteralAttr           = 21,
    GetLiteralAttr           = 22,
    SetCompiledCode          = 23,
    SendLiteral              = 24,
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
    RunLiteralFile           = 84,
    RunFile                  = 85,
    Send                     = 86,
    GetSelf                  = 87,
    GetBindingPrototype      = 88,
    GetBinding               = 89,
    SetConst                 = 90,
    GetConst                 = 91,
    SetAttr                  = 92,
    GetAttr                  = 93,
    LiteralConstExists       = 94,
    RunLiteralCode           = 95,
    SetPrototype             = 96,
    GetPrototype             = 97,
    LocalExists              = 98,
    GetCaller                = 99,
    LiteralRespondsTo        = 100,
    RespondsTo               = 101,
    LiteralAttrExists        = 102,
    SetOuterScope            = 103
}

/// Struct for storing information about a single instruction.
#[derive(Clone, Debug)]
pub struct Instruction {
    /// The type of instruction.
    pub instruction_type: InstructionType,

    /// The arguments of the instruction.
    pub arguments: Vec<u32>,

    /// The line from which the instruction originated.
    pub line: u32,

    /// The column from which the instruction originated.
    pub column: u32
}

impl Instruction {
    /// Returns a new Instruction.
    pub fn new(ins_type: InstructionType, arguments: Vec<u32>, line: u32,
               column: u32) -> Instruction {
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
            .ok_or(format!("undefined instruction argument {} for {:?}", index, self))
            .map(|num| num as usize)
    }
}
