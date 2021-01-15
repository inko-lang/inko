//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Allocate,
    AllocatePermanent,
    ArrayAllocate,
    ArrayAt,
    ArrayClear,
    ArrayLength,
    ArrayRemove,
    ArraySet,
    AttributeExists,
    BlockGetReceiver,
    ByteArrayAt,
    ByteArrayClear,
    ByteArrayEquals,
    ByteArrayFromArray,
    ByteArrayLength,
    ByteArrayRemove,
    ByteArraySet,
    ByteArrayToString,
    Close,
    CopyBlocks,
    CopyRegister,
    Exit,
    ExternalFunctionCall,
    ExternalFunctionLoad,
    FloatAdd,
    FloatCeil,
    FloatDiv,
    FloatEquals,
    FloatFloor,
    FloatGreater,
    FloatGreaterOrEqual,
    FloatIsInfinite,
    FloatIsNan,
    FloatMod,
    FloatMul,
    FloatRound,
    FloatSmaller,
    FloatSmallerOrEqual,
    FloatSub,
    FloatToBits,
    FloatToInteger,
    FloatToString,
    GeneratorAllocate,
    GeneratorResume,
    GeneratorValue,
    GeneratorYield,
    GetAttribute,
    GetAttributeInSelf,
    GetAttributeNames,
    GetBuiltinPrototype,
    GetFalse,
    GetGlobal,
    GetLocal,
    GetNil,
    GetParentLocal,
    GetPrototype,
    GetTrue,
    Goto,
    GotoIfFalse,
    GotoIfTrue,
    IntegerAdd,
    IntegerBitwiseAnd,
    IntegerBitwiseOr,
    IntegerBitwiseXor,
    IntegerDiv,
    IntegerEquals,
    IntegerGreater,
    IntegerGreaterOrEqual,
    IntegerMod,
    IntegerMul,
    IntegerShiftLeft,
    IntegerShiftRight,
    IntegerSmaller,
    IntegerSmallerOrEqual,
    IntegerSub,
    IntegerToFloat,
    IntegerToString,
    LocalExists,
    ModuleGet,
    ModuleLoad,
    MoveResult,
    ObjectEquals,
    Panic,
    ProcessAddDeferToCaller,
    ProcessCurrent,
    ProcessIdentifier,
    ProcessReceiveMessage,
    ProcessSendMessage,
    ProcessSetBlocking,
    ProcessSetPanicHandler,
    ProcessSetPinned,
    ProcessSpawn,
    ProcessSuspendCurrent,
    ProcessTerminateCurrent,
    Return,
    RunBlock,
    RunBlockWithReceiver,
    SetAttribute,
    SetBlock,
    SetDefaultPanicHandler,
    SetGlobal,
    SetLiteral,
    SetLiteralWide,
    SetLocal,
    SetParentLocal,
    StringByte,
    StringConcat,
    StringConcatArray,
    StringEquals,
    StringFormatDebug,
    StringLength,
    StringSize,
    StringSlice,
    StringToByteArray,
    StringToFloat,
    StringToInteger,
    StringToLower,
    StringToUpper,
    TailCall,
    Throw,
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
mod tests {
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
