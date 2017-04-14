//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum InstructionType {
    SetInteger,
    SetFloat,
    SetString,
    SetObject,
    SetArray,
    GetIntegerPrototype,
    GetFloatPrototype,
    GetStringPrototype,
    GetArrayPrototype,
    GetTruePrototype,
    GetFalsePrototype,
    GetMethodPrototype,
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
    DefMethod,
    RunBlock,
    IsError,
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
    FileSeek,
    ParseFile,
    FileParsed,
    GetBindingPrototype,
    GetBinding,
    SetConstant,
    GetConstant,
    SetAttribute,
    GetAttribute,
    SetPrototype,
    GetPrototype,
    LocalExists,
    RespondsTo,
    SpawnProcess,
    SendProcessMessage,
    ReceiveProcessMessage,
    GetCurrentPid,
    SetParentLocal,
    GetParentLocal,
    ErrorToInteger,
    FileReadExact,
    StdinReadExact,
    ObjectEquals,
    GetToplevel,
    GetNilPrototype,
    GetNil,
    LookupMethod,
    AttrExists,
    ConstExists,
    RemoveMethod,
    RemoveAttribute,
    GetMethods,
    GetMethodNames,
    GetAttributes,
    GetAttributeNames,
    MonotonicTimeNanoseconds,
    MonotonicTimeMilliseconds,
    RunBlockWithRest,
    GetGlobal,
    SetGlobal,
    SendMessage,
    ArrayPush,
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
    pub fn new(ins_type: InstructionType,
               arguments: Vec<usize>,
               line: u16)
               -> Instruction {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_instruction() -> Instruction {
        Instruction::new(InstructionType::SetInteger, vec![1, 2], 3)
    }

    #[test]
    fn test_new() {
        let ins = new_instruction();

        assert!(match ins.instruction_type {
            InstructionType::SetInteger => true,
            _ => false,
        });

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
}
