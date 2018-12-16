//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstructionType {
    SetLiteral,
    SetObject,
    SetArray,
    GetBuiltinPrototype,
    GetTrue,
    GetFalse,
    SetLocal,
    GetLocal,
    SetBlock,
    Return,
    GotoIfFalse,
    GotoIfTrue,
    Goto,
    RunBlock,
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
    ArraySet,
    ArrayAt,
    ArrayRemove,
    ArrayLength,
    ArrayClear,
    StringToLower,
    StringToUpper,
    StringEquals,
    StringToByteArray,
    StringLength,
    StringSize,
    StdoutWrite,
    StderrWrite,
    StdinRead,
    FileOpen,
    FileWrite,
    FileRead,
    FileFlush,
    FileSize,
    FileSeek,
    LoadModule,
    SetAttribute,
    GetAttribute,
    SetPrototype,
    GetPrototype,
    LocalExists,
    ProcessSpawn,
    ProcessSendMessage,
    ProcessReceiveMessage,
    ProcessCurrentPid,
    SetParentLocal,
    GetParentLocal,
    ObjectEquals,
    GetToplevel,
    GetNil,
    AttributeExists,
    RemoveAttribute,
    GetAttributeNames,
    TimeMonotonic,
    GetGlobal,
    SetGlobal,
    Throw,
    SetRegister,
    TailCall,
    ProcessSuspendCurrent,
    IntegerGreaterOrEqual,
    IntegerSmallerOrEqual,
    FloatGreaterOrEqual,
    FloatSmallerOrEqual,
    CopyBlocks,
    SetAttributeToObject,
    FloatIsNan,
    FloatIsInfinite,
    FloatFloor,
    FloatCeil,
    FloatRound,
    Drop,
    MoveToPool,
    StdoutFlush,
    StderrFlush,
    FileRemove,
    Panic,
    Exit,
    Platform,
    FileCopy,
    FileType,
    FileTime,
    TimeSystem,
    TimeSystemOffset,
    TimeSystemDst,
    DirectoryCreate,
    DirectoryRemove,
    DirectoryList,
    StringConcat,
    HasherNew,
    HasherWrite,
    HasherFinish,
    Stacktrace,
    ProcessTerminateCurrent,
    StringSlice,
    BlockMetadata,
    StringFormatDebug,
    StringConcatMultiple,
    ByteArrayFromArray,
    ByteArraySet,
    ByteArrayAt,
    ByteArrayRemove,
    ByteArrayLength,
    ByteArrayClear,
    ByteArrayEquals,
    ByteArrayToString,
    EnvGet,
    EnvSet,
    EnvVariables,
    EnvHomeDirectory,
    EnvTempDirectory,
    EnvGetWorkingDirectory,
    EnvSetWorkingDirectory,
    EnvArguments,
    EnvRemove,
    BlockGetReceiver,
    BlockSetReceiver,
    RunBlockWithReceiver,
    ProcessSetPanicHandler,
    ProcessAddDeferToCaller,
    SetDefaultPanicHandler,
    ProcessPinThread,
    ProcessUnpinThread,
    LibraryOpen,
    FunctionAttach,
    FunctionCall,
    PointerAttach,
    PointerRead,
    PointerWrite,
    PointerFromAddress,
    PointerAddress,
    ForeignTypeSize,
    ForeignTypeAlignment,
    StringToInteger,
    StringToFloat,
}

/// Struct for storing information about a single instruction.
#[derive(Debug)]
pub struct Instruction {
    /// The type of instruction.
    pub instruction_type: InstructionType,

    /// The arguments of the instruction.
    pub arguments: Vec<u16>,

    /// The line from which the instruction originated.
    pub line: u16,
}

impl Instruction {
    /// Returns a new Instruction.
    pub fn new(
        instruction_type: InstructionType,
        arguments: Vec<u16>,
        line: u16,
    ) -> Instruction {
        Instruction {
            instruction_type,
            arguments,
            line,
        }
    }

    /// Returns the value of an argument without performing any bounds checking.
    pub fn arg(&self, index: usize) -> usize {
        unsafe { *self.arguments.get_unchecked(index) as usize }
    }

    /// Returns the value of an argument as an Option.
    pub fn arg_opt(&self, index: usize) -> Option<usize> {
        self.arguments.get(index).map(|val| *val as usize)
    }

    pub fn boolean(&self, index: usize) -> bool {
        self.arg(index) == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_instruction() -> Instruction {
        Instruction::new(InstructionType::SetLiteral, vec![1, 2], 3)
    }

    #[test]
    fn test_new() {
        let ins = new_instruction();

        assert_eq!(ins.instruction_type, InstructionType::SetLiteral);
        assert_eq!(ins.arguments[0], 1);
        assert_eq!(ins.arguments[1], 2);
        assert_eq!(ins.line, 3);
    }

    #[test]
    fn test_arg() {
        let ins = new_instruction();

        assert_eq!(ins.arg(0), 1);
    }

    #[test]
    fn test_arg_opt_invalid() {
        let ins = new_instruction();

        assert!(ins.arg_opt(5).is_none());
    }

    #[test]
    fn test_arg_opt_valid() {
        let ins = new_instruction();

        assert!(ins.arg_opt(0).is_some());
        assert_eq!(ins.arg_opt(0).unwrap(), 1);
    }

    #[test]
    fn test_boolean() {
        let ins = new_instruction();

        assert!(ins.boolean(0));
    }
}
