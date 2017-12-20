//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstructionType {
    SetLiteral,
    SetObject,
    SetArray,
    GetIntegerPrototype,
    GetFloatPrototype,
    GetStringPrototype,
    GetArrayPrototype,
    GetBooleanPrototype,
    GetBlockPrototype,
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
    FileReadExact,
    StdinReadExact,
    ObjectEquals,
    GetToplevel,
    GetNil,
    AttributeExists,
    RemoveAttribute,
    GetAttributeNames,
    TimeMonotonicNanoseconds,
    TimeMonotonicMilliseconds,
    GetGlobal,
    SetGlobal,
    Throw,
    SetRegister,
    TailCall,
}

/// Struct for storing information about a single instruction.
#[derive(Debug)]
pub struct Instruction {
    /// The type of instruction.
    pub instruction_type: InstructionType,

    /// The arguments of the instruction.
    pub arguments: Vec<usize>,

    /// The line from which the instruction originated.
    pub line: u16,
}

impl Instruction {
    /// Returns a new Instruction.
    pub fn new(
        ins_type: InstructionType,
        arguments: Vec<usize>,
        line: u16,
    ) -> Instruction {
        Instruction {
            instruction_type: ins_type,
            arguments: arguments,
            line: line,
        }
    }

    /// Returns the value of an argument without performing any bounds checking.
    pub fn arg(&self, index: usize) -> usize {
        unsafe { *self.arguments.get_unchecked(index) }
    }

    /// Returns the value of an argument as an Option.
    pub fn arg_opt(&self, index: usize) -> Option<usize> {
        self.arguments.get(index).cloned()
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
