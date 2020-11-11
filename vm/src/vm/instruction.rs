//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    SetLiteral,
    SetLiteralWide,
    Allocate,
    AllocatePermanent,
    ArrayAllocate,
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
    ModuleLoad,
    SetAttribute,
    GetAttribute,
    GetPrototype,
    LocalExists,
    ProcessSpawn,
    ProcessSendMessage,
    ProcessReceiveMessage,
    ProcessCurrent,
    SetParentLocal,
    GetParentLocal,
    ObjectEquals,
    GetNil,
    AttributeExists,
    GetAttributeNames,
    TimeMonotonic,
    GetGlobal,
    SetGlobal,
    Throw,
    CopyRegister,
    TailCall,
    ProcessSuspendCurrent,
    IntegerGreaterOrEqual,
    IntegerSmallerOrEqual,
    FloatGreaterOrEqual,
    FloatSmallerOrEqual,
    CopyBlocks,
    FloatIsNan,
    FloatIsInfinite,
    FloatFloor,
    FloatCeil,
    FloatRound,
    Close,
    ProcessSetBlocking,
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
    DirectoryCreate,
    DirectoryRemove,
    DirectoryList,
    StringConcat,
    HasherNew,
    HasherWrite,
    HasherToHash,
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
    RunBlockWithReceiver,
    ProcessSetPanicHandler,
    ProcessAddDeferToCaller,
    SetDefaultPanicHandler,
    ProcessSetPinned,
    FFILibraryOpen,
    FFIFunctionAttach,
    FFIFunctionCall,
    FFIPointerAttach,
    FFIPointerRead,
    FFIPointerWrite,
    FFIPointerFromAddress,
    FFIPointerAddress,
    FFITypeSize,
    FFITypeAlignment,
    StringToInteger,
    StringToFloat,
    FloatToBits,
    ProcessIdentifier,
    SocketCreate,
    SocketWrite,
    SocketRead,
    SocketAccept,
    SocketReceiveFrom,
    SocketSendTo,
    SocketAddress,
    SocketGetOption,
    SocketSetOption,
    SocketBind,
    SocketListen,
    SocketConnect,
    SocketShutdown,
    RandomNumber,
    RandomRange,
    RandomBytes,
    StringByte,
    ModuleList,
    ModuleGet,
    ModuleInfo,
    GetAttributeInSelf,
    MoveResult,
    FilePath,
    GeneratorAllocate,
    GeneratorResume,
    GeneratorYield,
    GeneratorValue,
    GeneratorYielded,
}

/// A fixed-width VM instruction.
pub struct Instruction {
    /// The instruction opcode/type.
    pub opcode: Opcode,

    /// The line number of the instruction.
    pub line: u16,

    /// The arguments/operands of the instruction.
    ///
    /// This field is private so other code won't depend on this field having a
    /// particular shape.
    arguments: [u16; 6],
}

impl Instruction {
    pub fn new(opcode: Opcode, arguments: [u16; 6], line: u16) -> Self {
        Instruction {
            opcode,
            arguments,
            line,
        }
    }

    /// Returns the value of the given instruction argument.
    ///
    /// This method is always inlined to ensure bounds checking is optimised
    /// away when using literal index values.
    #[inline(always)]
    pub fn arg(&self, index: usize) -> u16 {
        self.arguments[index]
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;
    use std::mem::size_of;

    fn new_instruction() -> Instruction {
        Instruction::new(Opcode::SetLiteral, [1, 2, 0, 0, 0, 0], 3)
    }

    #[test]
    fn test_arg() {
        let ins = new_instruction();

        assert_eq!(ins.arg(0), 1);
    }

    #[test]
    fn test_type_size() {
        assert_eq!(size_of::<Instruction>(), 16);
    }
}
