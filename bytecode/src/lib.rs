//! Bytecode types shared between the compiler and VM.
use std::fmt;

/// A value is an owned reference.
pub const REF_OWNED: u16 = 0;

/// A value is a regular reference/borrow.
pub const REF_REF: u16 = 1;

/// A value is an atomic value.
pub const REF_ATOMIC: u16 = 2;

/// A value is an immediate or permanent value, i.e. a value that doesn't need
/// to be dropped.
pub const REF_PERMANENT: u16 = 3;

/// The bytes that every bytecode file must start with.
pub const SIGNATURE_BYTES: [u8; 4] = [105, 110, 107, 111]; // "inko"

/// The current version of the bytecode format.
pub const VERSION: u8 = 1;

/// The tag that marks the start of an integer constant.
pub const CONST_INTEGER: u8 = 0;

/// The tag that marks the start of a float constant.
pub const CONST_FLOAT: u8 = 1;

/// The tag that marks the start of a string constant.
pub const CONST_STRING: u8 = 2;

/// The tag that marks the start of an array constant.
pub const CONST_ARRAY: u8 = 3;

/// Enum containing all possible instruction types.
///
/// When adding new opcodes, you must also add them to the `Opcode::from_byte`
/// method.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[repr(u8)]
pub enum Opcode {
    Allocate,
    ArrayAllocate,
    ArrayClear,
    ArrayDrop,
    ArrayGet,
    ArrayLength,
    ArrayPop,
    ArrayPush,
    ArrayRemove,
    ArraySet,
    Branch,
    BranchResult,
    BuiltinFunctionCall,
    ByteArrayAllocate,
    ByteArrayClear,
    ByteArrayClone,
    ByteArrayDrop,
    ByteArrayEquals,
    ByteArrayGet,
    ByteArrayLength,
    ByteArrayPop,
    ByteArrayPush,
    ByteArrayRemove,
    ByteArraySet,
    CallDynamic,
    CallVirtual,
    CheckRefs,
    Decrement,
    DecrementAtomic,
    Exit,
    FloatAdd,
    FloatCeil,
    FloatClone,
    FloatDiv,
    FloatEq,
    FloatFloor,
    FloatGe,
    FloatGt,
    FloatIsInf,
    FloatIsNan,
    FloatLe,
    FloatLt,
    FloatMod,
    FloatMul,
    FloatRound,
    FloatSub,
    FloatToInt,
    FloatToString,
    Free,
    FutureDrop,
    FutureGet,
    FutureGetFor,
    GetClass,
    GetConstant,
    GetFalse,
    GetField,
    GetModule,
    GetNil,
    GetTrue,
    GetUndefined,
    Goto,
    Increment,
    IntAdd,
    IntBitAnd,
    IntBitOr,
    IntBitXor,
    IntClone,
    IntDiv,
    IntEq,
    IntGe,
    IntGt,
    IntLe,
    IntLt,
    IntMod,
    IntMul,
    IntPow,
    IntShl,
    IntShr,
    IntSub,
    IntToFloat,
    IntToString,
    IsUndefined,
    MoveRegister,
    MoveResult,
    ObjectEq,
    Panic,
    ProcessAllocate,
    ProcessFinishTask,
    ProcessGetField,
    ProcessSend,
    ProcessSendAsync,
    ProcessSetField,
    ProcessSuspend,
    ProcessWriteResult,
    Reduce,
    RefKind,
    Return,
    SetField,
    StringByte,
    StringConcat,
    StringDrop,
    StringEq,
    StringSize,
    JumpTable,
    Throw,
    Push,
    Pop,
    FuturePoll,
    IntBitNot,
    IntRotateLeft,
    IntRotateRight,
}

impl Opcode {
    pub fn from_byte(byte: u8) -> Result<Opcode, String> {
        let opcode = match byte {
            0 => Opcode::Allocate,
            1 => Opcode::ArrayAllocate,
            2 => Opcode::ArrayClear,
            3 => Opcode::ArrayDrop,
            4 => Opcode::ArrayGet,
            5 => Opcode::ArrayLength,
            6 => Opcode::ArrayPop,
            7 => Opcode::ArrayPush,
            8 => Opcode::ArrayRemove,
            9 => Opcode::ArraySet,
            10 => Opcode::BuiltinFunctionCall,
            11 => Opcode::ByteArrayAllocate,
            12 => Opcode::ByteArrayClear,
            13 => Opcode::ByteArrayClone,
            14 => Opcode::ByteArrayDrop,
            15 => Opcode::ByteArrayEquals,
            16 => Opcode::ByteArrayGet,
            17 => Opcode::ByteArrayLength,
            18 => Opcode::ByteArrayPop,
            19 => Opcode::ByteArrayPush,
            20 => Opcode::ByteArrayRemove,
            21 => Opcode::ByteArraySet,
            22 => Opcode::CallDynamic,
            23 => Opcode::CallVirtual,
            24 => Opcode::CheckRefs,
            25 => Opcode::MoveRegister,
            26 => Opcode::Decrement,
            27 => Opcode::Exit,
            28 => Opcode::FloatAdd,
            29 => Opcode::FloatCeil,
            30 => Opcode::FloatClone,
            31 => Opcode::FloatDiv,
            32 => Opcode::FloatEq,
            33 => Opcode::FloatFloor,
            34 => Opcode::FloatGe,
            35 => Opcode::FloatGt,
            36 => Opcode::FloatIsInf,
            37 => Opcode::FloatIsNan,
            38 => Opcode::FloatLe,
            39 => Opcode::FloatLt,
            40 => Opcode::FloatMod,
            41 => Opcode::FloatMul,
            42 => Opcode::FloatRound,
            43 => Opcode::FloatSub,
            44 => Opcode::FloatToInt,
            45 => Opcode::FloatToString,
            46 => Opcode::FutureDrop,
            47 => Opcode::FutureGet,
            48 => Opcode::FutureGetFor,
            49 => Opcode::GetConstant,
            50 => Opcode::GetFalse,
            51 => Opcode::GetField,
            52 => Opcode::GetClass,
            53 => Opcode::GetModule,
            54 => Opcode::GetNil,
            55 => Opcode::GetTrue,
            56 => Opcode::GetUndefined,
            57 => Opcode::Goto,
            58 => Opcode::Branch,
            59 => Opcode::BranchResult,
            60 => Opcode::Increment,
            61 => Opcode::IntAdd,
            62 => Opcode::IntBitAnd,
            63 => Opcode::IntBitOr,
            64 => Opcode::IntBitXor,
            65 => Opcode::IntClone,
            66 => Opcode::IntDiv,
            67 => Opcode::IntEq,
            68 => Opcode::IntGe,
            69 => Opcode::IntGt,
            70 => Opcode::IntLe,
            71 => Opcode::IntLt,
            72 => Opcode::IntMod,
            73 => Opcode::IntMul,
            74 => Opcode::IntPow,
            75 => Opcode::IntShl,
            76 => Opcode::IntShr,
            77 => Opcode::IntSub,
            78 => Opcode::IntToFloat,
            79 => Opcode::IntToString,
            80 => Opcode::IsUndefined,
            81 => Opcode::RefKind,
            82 => Opcode::MoveResult,
            83 => Opcode::ObjectEq,
            84 => Opcode::Panic,
            85 => Opcode::ProcessAllocate,
            86 => Opcode::ProcessGetField,
            87 => Opcode::ProcessSendAsync,
            88 => Opcode::ProcessSend,
            89 => Opcode::ProcessSetField,
            90 => Opcode::ProcessSuspend,
            91 => Opcode::ProcessWriteResult,
            92 => Opcode::Free,
            93 => Opcode::Reduce,
            94 => Opcode::Return,
            95 => Opcode::SetField,
            96 => Opcode::StringByte,
            97 => Opcode::StringConcat,
            98 => Opcode::StringDrop,
            99 => Opcode::StringEq,
            100 => Opcode::StringSize,
            101 => Opcode::Throw,
            102 => Opcode::DecrementAtomic,
            103 => Opcode::ProcessFinishTask,
            104 => Opcode::JumpTable,
            105 => Opcode::Push,
            106 => Opcode::Pop,
            107 => Opcode::FuturePoll,
            108 => Opcode::IntBitNot,
            109 => Opcode::IntRotateLeft,
            110 => Opcode::IntRotateRight,
            _ => return Err(format!("The opcode {} is invalid", byte)),
        };

        Ok(opcode)
    }

    pub fn to_int(self) -> u8 {
        // This must be kept in sync with `Opcode::from_byte()`.
        match self {
            Opcode::Allocate => 0,
            Opcode::ArrayAllocate => 1,
            Opcode::ArrayClear => 2,
            Opcode::ArrayDrop => 3,
            Opcode::ArrayGet => 4,
            Opcode::ArrayLength => 5,
            Opcode::ArrayPop => 6,
            Opcode::ArrayPush => 7,
            Opcode::ArrayRemove => 8,
            Opcode::ArraySet => 9,
            Opcode::BuiltinFunctionCall => 10,
            Opcode::ByteArrayAllocate => 11,
            Opcode::ByteArrayClear => 12,
            Opcode::ByteArrayClone => 13,
            Opcode::ByteArrayDrop => 14,
            Opcode::ByteArrayEquals => 15,
            Opcode::ByteArrayGet => 16,
            Opcode::ByteArrayLength => 17,
            Opcode::ByteArrayPop => 18,
            Opcode::ByteArrayPush => 19,
            Opcode::ByteArrayRemove => 20,
            Opcode::ByteArraySet => 21,
            Opcode::CallDynamic => 22,
            Opcode::CallVirtual => 23,
            Opcode::CheckRefs => 24,
            Opcode::MoveRegister => 25,
            Opcode::Decrement => 26,
            Opcode::Exit => 27,
            Opcode::FloatAdd => 28,
            Opcode::FloatCeil => 29,
            Opcode::FloatClone => 30,
            Opcode::FloatDiv => 31,
            Opcode::FloatEq => 32,
            Opcode::FloatFloor => 33,
            Opcode::FloatGe => 34,
            Opcode::FloatGt => 35,
            Opcode::FloatIsInf => 36,
            Opcode::FloatIsNan => 37,
            Opcode::FloatLe => 38,
            Opcode::FloatLt => 39,
            Opcode::FloatMod => 40,
            Opcode::FloatMul => 41,
            Opcode::FloatRound => 42,
            Opcode::FloatSub => 43,
            Opcode::FloatToInt => 44,
            Opcode::FloatToString => 45,
            Opcode::FutureDrop => 46,
            Opcode::FutureGet => 47,
            Opcode::FutureGetFor => 48,
            Opcode::GetConstant => 49,
            Opcode::GetFalse => 50,
            Opcode::GetField => 51,
            Opcode::GetClass => 52,
            Opcode::GetModule => 53,
            Opcode::GetNil => 54,
            Opcode::GetTrue => 55,
            Opcode::GetUndefined => 56,
            Opcode::Goto => 57,
            Opcode::Branch => 58,
            Opcode::BranchResult => 59,
            Opcode::Increment => 60,
            Opcode::IntAdd => 61,
            Opcode::IntBitAnd => 62,
            Opcode::IntBitOr => 63,
            Opcode::IntBitXor => 64,
            Opcode::IntClone => 65,
            Opcode::IntDiv => 66,
            Opcode::IntEq => 67,
            Opcode::IntGe => 68,
            Opcode::IntGt => 69,
            Opcode::IntLe => 70,
            Opcode::IntLt => 71,
            Opcode::IntMod => 72,
            Opcode::IntMul => 73,
            Opcode::IntPow => 74,
            Opcode::IntShl => 75,
            Opcode::IntShr => 76,
            Opcode::IntSub => 77,
            Opcode::IntToFloat => 78,
            Opcode::IntToString => 79,
            Opcode::IsUndefined => 80,
            Opcode::RefKind => 81,
            Opcode::MoveResult => 82,
            Opcode::ObjectEq => 83,
            Opcode::Panic => 84,
            Opcode::ProcessAllocate => 85,
            Opcode::ProcessGetField => 86,
            Opcode::ProcessSendAsync => 87,
            Opcode::ProcessSend => 88,
            Opcode::ProcessSetField => 89,
            Opcode::ProcessSuspend => 90,
            Opcode::ProcessWriteResult => 91,
            Opcode::Free => 92,
            Opcode::Reduce => 93,
            Opcode::Return => 94,
            Opcode::SetField => 95,
            Opcode::StringByte => 96,
            Opcode::StringConcat => 97,
            Opcode::StringDrop => 98,
            Opcode::StringEq => 99,
            Opcode::StringSize => 100,
            Opcode::Throw => 101,
            Opcode::DecrementAtomic => 102,
            Opcode::ProcessFinishTask => 103,
            Opcode::JumpTable => 104,
            Opcode::Push => 105,
            Opcode::Pop => 106,
            Opcode::FuturePoll => 107,
            Opcode::IntBitNot => 108,
            Opcode::IntRotateLeft => 109,
            Opcode::IntRotateRight => 110,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Opcode::Allocate => "allocate",
            Opcode::ArrayAllocate => "array_allocate",
            Opcode::ArrayClear => "array_clear",
            Opcode::ArrayDrop => "array_drop",
            Opcode::ArrayGet => "array_get",
            Opcode::ArrayLength => "array_length",
            Opcode::ArrayPop => "array_pop",
            Opcode::ArrayPush => "array_push",
            Opcode::ArrayRemove => "array_remove",
            Opcode::ArraySet => "array_set",
            Opcode::BuiltinFunctionCall => "builtin_function_call",
            Opcode::ByteArrayAllocate => "byte_array_allocate",
            Opcode::ByteArrayClear => "byte_array_clear",
            Opcode::ByteArrayClone => "byte_array_clone",
            Opcode::ByteArrayDrop => "byte_array_drop",
            Opcode::ByteArrayEquals => "byte_array_equals",
            Opcode::ByteArrayGet => "byte_array_get",
            Opcode::ByteArrayLength => "byte_array_length",
            Opcode::ByteArrayPop => "byte_array_pop",
            Opcode::ByteArrayPush => "byte_array_push",
            Opcode::ByteArrayRemove => "byte_array_remove",
            Opcode::ByteArraySet => "byte_array_set",
            Opcode::CallDynamic => "call_dynamic",
            Opcode::CallVirtual => "call_virtual",
            Opcode::CheckRefs => "check_refs",
            Opcode::MoveRegister => "move_register",
            Opcode::Decrement => "decrement",
            Opcode::Exit => "exit",
            Opcode::FloatAdd => "float_add",
            Opcode::FloatCeil => "float_ceil",
            Opcode::FloatClone => "float_clone",
            Opcode::FloatDiv => "float_div",
            Opcode::FloatEq => "float_eq",
            Opcode::FloatFloor => "float_floor",
            Opcode::FloatGe => "float_ge",
            Opcode::FloatGt => "float_gt",
            Opcode::FloatIsInf => "float_is_inf",
            Opcode::FloatIsNan => "float_is_nan",
            Opcode::FloatLe => "float_le",
            Opcode::FloatLt => "float_lt",
            Opcode::FloatMod => "float_mod",
            Opcode::FloatMul => "float_mul",
            Opcode::FloatRound => "float_round",
            Opcode::FloatSub => "float_sub",
            Opcode::FloatToInt => "float_to_int",
            Opcode::FloatToString => "float_to_string",
            Opcode::FutureDrop => "future_drop",
            Opcode::FutureGet => "future_get",
            Opcode::FutureGetFor => "future_get_for",
            Opcode::GetConstant => "get_constant",
            Opcode::GetFalse => "get_false",
            Opcode::GetField => "get_field",
            Opcode::GetClass => "get_class",
            Opcode::GetModule => "get_module",
            Opcode::GetNil => "get_nil",
            Opcode::GetTrue => "get_true",
            Opcode::GetUndefined => "get_undefined",
            Opcode::Goto => "goto",
            Opcode::Branch => "branch",
            Opcode::BranchResult => "branch_result",
            Opcode::Increment => "increment",
            Opcode::IntAdd => "int_add",
            Opcode::IntBitAnd => "int_bit_and",
            Opcode::IntBitOr => "int_bit_or",
            Opcode::IntBitXor => "int_bit_xor",
            Opcode::IntClone => "int_clone",
            Opcode::IntDiv => "int_div",
            Opcode::IntEq => "int_eq",
            Opcode::IntGe => "int_ge",
            Opcode::IntGt => "int_gt",
            Opcode::IntLe => "int_le",
            Opcode::IntLt => "int_lt",
            Opcode::IntMod => "int_mod",
            Opcode::IntMul => "int_mul",
            Opcode::IntPow => "int_pow",
            Opcode::IntShl => "int_shl",
            Opcode::IntShr => "int_shr",
            Opcode::IntSub => "int_sub",
            Opcode::IntToFloat => "int_to_float",
            Opcode::IntToString => "int_to_string",
            Opcode::RefKind => "is_ref",
            Opcode::MoveResult => "move_result",
            Opcode::ObjectEq => "object_eq",
            Opcode::Panic => "panic",
            Opcode::ProcessAllocate => "process_allocate",
            Opcode::ProcessGetField => "process_get_field",
            Opcode::ProcessSendAsync => "process_send_async",
            Opcode::ProcessSend => "process_send",
            Opcode::ProcessSetField => "process_set_field",
            Opcode::ProcessSuspend => "process_suspend",
            Opcode::ProcessWriteResult => "process_write_result",
            Opcode::Free => "free",
            Opcode::Reduce => "reduce",
            Opcode::Return => "return",
            Opcode::SetField => "set_field",
            Opcode::StringByte => "string_byte",
            Opcode::StringConcat => "string_concat",
            Opcode::StringDrop => "string_drop",
            Opcode::StringEq => "string_eq",
            Opcode::StringSize => "string_size",
            Opcode::Throw => "throw",
            Opcode::IsUndefined => "is_undefined",
            Opcode::DecrementAtomic => "decrement_atomic",
            Opcode::ProcessFinishTask => "process_finish_task",
            Opcode::JumpTable => "jump_table",
            Opcode::Push => "push",
            Opcode::Pop => "pop",
            Opcode::FuturePoll => "future_poll",
            Opcode::IntBitNot => "int_bit_not",
            Opcode::IntRotateLeft => "int_rotate_left",
            Opcode::IntRotateRight => "int_rotate_right",
        }
    }

    pub fn writes(self) -> bool {
        // This list doesn't have to be exhaustive, as long as we cover the
        // instructions directly exposed to the standard library.
        !matches!(
            self,
            Opcode::ArrayClear
                | Opcode::ArrayDrop
                | Opcode::ArrayPush
                | Opcode::ByteArrayClear
                | Opcode::ByteArrayDrop
                | Opcode::ByteArrayPush
                | Opcode::Exit
                | Opcode::Panic
                | Opcode::ProcessSuspend
                | Opcode::StringDrop
        )
    }

    pub fn arity(self) -> usize {
        match self {
            Opcode::Allocate => 3,
            Opcode::ArrayAllocate => 1,
            Opcode::ArrayClear => 1,
            Opcode::ArrayDrop => 1,
            Opcode::ArrayGet => 3,
            Opcode::ArrayLength => 2,
            Opcode::ArrayPop => 2,
            Opcode::ArrayPush => 2,
            Opcode::ArrayRemove => 3,
            Opcode::ArraySet => 4,
            Opcode::Branch => 3,
            Opcode::BranchResult => 2,
            Opcode::BuiltinFunctionCall => 4,
            Opcode::ByteArrayAllocate => 1,
            Opcode::ByteArrayClear => 1,
            Opcode::ByteArrayClone => 2,
            Opcode::ByteArrayDrop => 1,
            Opcode::ByteArrayEquals => 3,
            Opcode::ByteArrayGet => 3,
            Opcode::ByteArrayLength => 2,
            Opcode::ByteArrayPop => 2,
            Opcode::ByteArrayPush => 2,
            Opcode::ByteArrayRemove => 3,
            Opcode::ByteArraySet => 4,
            Opcode::CallDynamic => 5,
            Opcode::CallVirtual => 4,
            Opcode::CheckRefs => 1,
            Opcode::Decrement => 1,
            Opcode::DecrementAtomic => 2,
            Opcode::Exit => 1,
            Opcode::FloatAdd => 3,
            Opcode::FloatCeil => 2,
            Opcode::FloatClone => 2,
            Opcode::FloatDiv => 3,
            Opcode::FloatEq => 3,
            Opcode::FloatFloor => 2,
            Opcode::FloatGe => 3,
            Opcode::FloatGt => 3,
            Opcode::FloatIsInf => 2,
            Opcode::FloatIsNan => 2,
            Opcode::FloatLe => 3,
            Opcode::FloatLt => 3,
            Opcode::FloatMod => 3,
            Opcode::FloatMul => 3,
            Opcode::FloatRound => 3,
            Opcode::FloatSub => 3,
            Opcode::FloatToInt => 2,
            Opcode::FloatToString => 2,
            Opcode::Free => 1,
            Opcode::FutureDrop => 2,
            Opcode::FutureGet => 1,
            Opcode::FutureGetFor => 2,
            Opcode::GetClass => 3,
            Opcode::GetConstant => 3,
            Opcode::GetFalse => 1,
            Opcode::GetField => 3,
            Opcode::GetModule => 3,
            Opcode::GetNil => 1,
            Opcode::GetTrue => 1,
            Opcode::GetUndefined => 1,
            Opcode::Goto => 1,
            Opcode::Increment => 2,
            Opcode::IntAdd => 3,
            Opcode::IntBitAnd => 3,
            Opcode::IntBitOr => 3,
            Opcode::IntBitXor => 3,
            Opcode::IntClone => 2,
            Opcode::IntDiv => 3,
            Opcode::IntEq => 3,
            Opcode::IntGe => 3,
            Opcode::IntGt => 3,
            Opcode::IntLe => 3,
            Opcode::IntLt => 3,
            Opcode::IntMod => 3,
            Opcode::IntMul => 3,
            Opcode::IntPow => 3,
            Opcode::IntShl => 3,
            Opcode::IntShr => 3,
            Opcode::IntSub => 3,
            Opcode::IntToFloat => 2,
            Opcode::IntToString => 2,
            Opcode::IsUndefined => 2,
            Opcode::MoveRegister => 2,
            Opcode::MoveResult => 1,
            Opcode::ObjectEq => 3,
            Opcode::Panic => 1,
            Opcode::ProcessAllocate => 3,
            Opcode::ProcessFinishTask => 1,
            Opcode::ProcessGetField => 3,
            Opcode::ProcessSend => 3,
            Opcode::ProcessSendAsync => 3,
            Opcode::ProcessSetField => 3,
            Opcode::ProcessSuspend => 1,
            Opcode::ProcessWriteResult => 3,
            Opcode::Reduce => 1,
            Opcode::RefKind => 2,
            Opcode::Return => 1,
            Opcode::SetField => 3,
            Opcode::StringByte => 3,
            Opcode::StringConcat => 1,
            Opcode::StringDrop => 1,
            Opcode::StringEq => 3,
            Opcode::StringSize => 2,
            Opcode::JumpTable => 2,
            Opcode::Throw => 2,
            Opcode::Push => 1,
            Opcode::Pop => 1,
            Opcode::FuturePoll => 2,
            Opcode::IntBitNot => 2,
            Opcode::IntRotateLeft => 3,
            Opcode::IntRotateRight => 3,
        }
    }

    pub fn rewind_before_call(self) -> bool {
        matches!(
            self,
            Opcode::BuiltinFunctionCall
                | Opcode::FutureGet
                | Opcode::FutureGetFor
        )
    }
}

/// A fixed-width VM instruction.
pub struct Instruction {
    /// The instruction opcode/type.
    pub opcode: Opcode,

    /// The arguments/operands of the instruction.
    ///
    /// This field is private so other code won't depend on this field having a
    /// particular shape.
    pub arguments: [u16; 5],
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut fmt = f.debug_tuple(self.opcode.name());

        for index in 0..self.opcode.arity() {
            fmt.field(&self.arguments[index]);
        }

        fmt.finish()
    }
}

impl Instruction {
    pub fn new(opcode: Opcode, arguments: [u16; 5]) -> Self {
        Instruction { opcode, arguments }
    }

    pub fn zero(opcode: Opcode) -> Self {
        Self::new(opcode, [0, 0, 0, 0, 0])
    }

    pub fn one(opcode: Opcode, arg0: u16) -> Self {
        Self::new(opcode, [arg0, 0, 0, 0, 0])
    }

    pub fn two(opcode: Opcode, arg0: u16, arg1: u16) -> Self {
        Self::new(opcode, [arg0, arg1, 0, 0, 0])
    }

    pub fn three(opcode: Opcode, arg0: u16, arg1: u16, arg2: u16) -> Self {
        Self::new(opcode, [arg0, arg1, arg2, 0, 0])
    }

    pub fn four(
        opcode: Opcode,
        arg0: u16,
        arg1: u16,
        arg2: u16,
        arg3: u16,
    ) -> Self {
        Self::new(opcode, [arg0, arg1, arg2, arg3, 0])
    }

    /// Returns the value of the given instruction argument.
    ///
    /// This method is always inlined to ensure bounds checking is optimised
    /// away when using literal index values.
    #[inline(always)]
    pub fn arg(&self, index: usize) -> u16 {
        self.arguments[index]
    }

    #[inline(always)]
    pub fn u32_arg(&self, one: usize, two: usize) -> u32 {
        let arg1 = u16::to_le_bytes(self.arg(one));
        let arg2 = u16::to_le_bytes(self.arg(two));

        u32::from_le_bytes([arg1[0], arg1[1], arg2[0], arg2[1]])
    }

    #[inline(always)]
    pub fn u64_arg(
        &self,
        one: usize,
        two: usize,
        three: usize,
        four: usize,
    ) -> u64 {
        let arg1 = u16::to_le_bytes(self.arg(one));
        let arg2 = u16::to_le_bytes(self.arg(two));
        let arg3 = u16::to_le_bytes(self.arg(three));
        let arg4 = u16::to_le_bytes(self.arg(four));

        u64::from_le_bytes([
            arg1[0], arg1[1], arg2[0], arg2[1], arg3[0], arg3[1], arg4[0],
            arg4[1],
        ])
    }
}

#[repr(u16)]
#[derive(Copy, Clone)]
pub enum BuiltinFunction {
    ByteArrayDrainToString,
    ByteArrayToString,
    ChildProcessDrop,
    ChildProcessSpawn,
    ChildProcessStderrClose,
    ChildProcessStderrRead,
    ChildProcessStdinClose,
    ChildProcessStdinFlush,
    ChildProcessStdinWriteBytes,
    ChildProcessStdinWriteString,
    ChildProcessStdoutClose,
    ChildProcessStdoutRead,
    ChildProcessTryWait,
    ChildProcessWait,
    DirectoryCreate,
    DirectoryCreateRecursive,
    DirectoryList,
    DirectoryRemove,
    DirectoryRemoveRecursive,
    EnvArguments,
    EnvExecutable,
    EnvGet,
    EnvGetWorkingDirectory,
    EnvHomeDirectory,
    EnvPlatform,
    EnvSetWorkingDirectory,
    EnvTempDirectory,
    EnvVariables,
    FFIFunctionAttach,
    FFIFunctionCall,
    FFIFunctionDrop,
    FFILibraryDrop,
    FFILibraryOpen,
    FFIPointerAddress,
    FFIPointerAttach,
    FFIPointerFromAddress,
    FFIPointerRead,
    FFIPointerWrite,
    FFITypeAlignment,
    FFITypeSize,
    FileCopy,
    FileDrop,
    FileFlush,
    FileOpenAppendOnly,
    FileOpenReadAppend,
    FileOpenReadOnly,
    FileOpenReadWrite,
    FileOpenWriteOnly,
    FileRead,
    FileRemove,
    FileSeek,
    FileSize,
    FileWriteBytes,
    FileWriteString,
    HasherDrop,
    HasherNew,
    HasherToHash,
    HasherWriteInt,
    PathAccessedAt,
    PathCreatedAt,
    PathExists,
    PathIsDirectory,
    PathIsFile,
    PathModifiedAt,
    ProcessStacktraceDrop,
    ProcessCallFrameLine,
    ProcessCallFrameName,
    ProcessCallFramePath,
    ProcessStacktrace,
    RandomBytes,
    RandomFloat,
    RandomFloatRange,
    RandomInt,
    RandomIntRange,
    SocketAcceptIp,
    SocketAcceptUnix,
    SocketAddressPairAddress,
    SocketAddressPairDrop,
    SocketAddressPairPort,
    SocketAllocateIpv4,
    SocketAllocateIpv6,
    SocketAllocateUnix,
    SocketBind,
    SocketConnect,
    SocketDrop,
    SocketGetBroadcast,
    SocketGetKeepalive,
    SocketGetLinger,
    SocketGetNodelay,
    SocketGetOnlyV6,
    SocketGetRecvSize,
    SocketGetReuseAddress,
    SocketGetReusePort,
    SocketGetSendSize,
    SocketGetTtl,
    SocketListen,
    SocketLocalAddress,
    SocketPeerAddress,
    SocketRead,
    SocketReceiveFrom,
    SocketSendBytesTo,
    SocketSendStringTo,
    SocketSetBroadcast,
    SocketSetKeepalive,
    SocketSetLinger,
    SocketSetNodelay,
    SocketSetOnlyV6,
    SocketSetRecvSize,
    SocketSetReuseAddress,
    SocketSetReusePort,
    SocketSetSendSize,
    SocketSetTtl,
    SocketShutdownRead,
    SocketShutdownReadWrite,
    SocketShutdownWrite,
    SocketTryClone,
    SocketWriteBytes,
    SocketWriteString,
    StderrFlush,
    StderrWriteBytes,
    StderrWriteString,
    StdinRead,
    StdoutFlush,
    StdoutWriteBytes,
    StdoutWriteString,
    StringToByteArray,
    StringToFloat,
    StringToInt,
    StringToLower,
    StringToUpper,
    TimeMonotonic,
    TimeSystem,
    TimeSystemOffset,
    CpuCores,
    StringCharacters,
    StringCharactersNext,
    StringCharactersDrop,
    StringConcatArray,
    ArrayReserve,
    ArrayCapacity,
    ProcessStacktraceLength,
    FloatToBits,
    FloatFromBits,
    RandomNew,
    RandomFromInt,
    RandomDrop,
    StringSliceBytes,
}

impl BuiltinFunction {
    pub fn to_int(self) -> u16 {
        match self {
            BuiltinFunction::ByteArrayDrainToString => 0,
            BuiltinFunction::ByteArrayToString => 1,
            BuiltinFunction::ChildProcessDrop => 2,
            BuiltinFunction::ChildProcessSpawn => 3,
            BuiltinFunction::ChildProcessStderrClose => 4,
            BuiltinFunction::ChildProcessStderrRead => 5,
            BuiltinFunction::ChildProcessStdinClose => 6,
            BuiltinFunction::ChildProcessStdinFlush => 7,
            BuiltinFunction::ChildProcessStdinWriteBytes => 8,
            BuiltinFunction::ChildProcessStdinWriteString => 9,
            BuiltinFunction::ChildProcessStdoutClose => 10,
            BuiltinFunction::ChildProcessStdoutRead => 11,
            BuiltinFunction::ChildProcessTryWait => 12,
            BuiltinFunction::ChildProcessWait => 13,
            BuiltinFunction::EnvArguments => 14,
            BuiltinFunction::EnvExecutable => 15,
            BuiltinFunction::EnvGet => 16,
            BuiltinFunction::EnvGetWorkingDirectory => 17,
            BuiltinFunction::EnvHomeDirectory => 18,
            BuiltinFunction::EnvPlatform => 19,
            BuiltinFunction::EnvSetWorkingDirectory => 20,
            BuiltinFunction::EnvTempDirectory => 21,
            BuiltinFunction::EnvVariables => 22,
            BuiltinFunction::FFIFunctionAttach => 23,
            BuiltinFunction::FFIFunctionCall => 24,
            BuiltinFunction::FFIFunctionDrop => 25,
            BuiltinFunction::FFILibraryDrop => 26,
            BuiltinFunction::FFILibraryOpen => 27,
            BuiltinFunction::FFIPointerAddress => 28,
            BuiltinFunction::FFIPointerAttach => 29,
            BuiltinFunction::FFIPointerFromAddress => 30,
            BuiltinFunction::FFIPointerRead => 31,
            BuiltinFunction::FFIPointerWrite => 32,
            BuiltinFunction::FFITypeAlignment => 33,
            BuiltinFunction::FFITypeSize => 34,
            BuiltinFunction::DirectoryCreate => 35,
            BuiltinFunction::DirectoryCreateRecursive => 36,
            BuiltinFunction::DirectoryList => 37,
            BuiltinFunction::DirectoryRemove => 38,
            BuiltinFunction::DirectoryRemoveRecursive => 39,
            BuiltinFunction::FileCopy => 40,
            BuiltinFunction::FileDrop => 41,
            BuiltinFunction::FileFlush => 42,
            BuiltinFunction::FileOpenAppendOnly => 43,
            BuiltinFunction::FileOpenReadAppend => 44,
            BuiltinFunction::FileOpenReadOnly => 45,
            BuiltinFunction::FileOpenReadWrite => 46,
            BuiltinFunction::FileOpenWriteOnly => 47,
            BuiltinFunction::FileRead => 48,
            BuiltinFunction::FileRemove => 49,
            BuiltinFunction::FileSeek => 50,
            BuiltinFunction::FileSize => 51,
            BuiltinFunction::FileWriteBytes => 52,
            BuiltinFunction::FileWriteString => 53,
            BuiltinFunction::PathAccessedAt => 54,
            BuiltinFunction::PathCreatedAt => 55,
            BuiltinFunction::PathExists => 56,
            BuiltinFunction::PathIsDirectory => 57,
            BuiltinFunction::PathIsFile => 58,
            BuiltinFunction::PathModifiedAt => 59,
            BuiltinFunction::HasherDrop => 60,
            BuiltinFunction::HasherNew => 61,
            BuiltinFunction::HasherToHash => 62,
            BuiltinFunction::HasherWriteInt => 63,
            BuiltinFunction::ProcessStacktraceDrop => 64,
            BuiltinFunction::ProcessCallFrameLine => 65,
            BuiltinFunction::ProcessCallFrameName => 66,
            BuiltinFunction::ProcessCallFramePath => 67,
            BuiltinFunction::ProcessStacktrace => 68,
            BuiltinFunction::RandomBytes => 69,
            BuiltinFunction::RandomFloat => 70,
            BuiltinFunction::RandomFloatRange => 71,
            BuiltinFunction::RandomInt => 72,
            BuiltinFunction::RandomIntRange => 73,
            BuiltinFunction::SocketAcceptIp => 74,
            BuiltinFunction::SocketAcceptUnix => 75,
            BuiltinFunction::SocketAddressPairAddress => 76,
            BuiltinFunction::SocketAddressPairDrop => 77,
            BuiltinFunction::SocketAddressPairPort => 78,
            BuiltinFunction::SocketAllocateIpv4 => 79,
            BuiltinFunction::SocketAllocateIpv6 => 80,
            BuiltinFunction::SocketAllocateUnix => 81,
            BuiltinFunction::SocketBind => 82,
            BuiltinFunction::SocketConnect => 83,
            BuiltinFunction::SocketDrop => 84,
            BuiltinFunction::SocketGetBroadcast => 85,
            BuiltinFunction::SocketGetKeepalive => 86,
            BuiltinFunction::SocketGetLinger => 87,
            BuiltinFunction::SocketGetNodelay => 88,
            BuiltinFunction::SocketGetOnlyV6 => 89,
            BuiltinFunction::SocketGetRecvSize => 90,
            BuiltinFunction::SocketGetReuseAddress => 91,
            BuiltinFunction::SocketGetReusePort => 92,
            BuiltinFunction::SocketGetSendSize => 93,
            BuiltinFunction::SocketGetTtl => 94,
            BuiltinFunction::SocketListen => 95,
            BuiltinFunction::SocketLocalAddress => 96,
            BuiltinFunction::SocketPeerAddress => 97,
            BuiltinFunction::SocketRead => 98,
            BuiltinFunction::SocketReceiveFrom => 99,
            BuiltinFunction::SocketSendBytesTo => 100,
            BuiltinFunction::SocketSendStringTo => 101,
            BuiltinFunction::SocketSetBroadcast => 102,
            BuiltinFunction::SocketSetKeepalive => 103,
            BuiltinFunction::SocketSetLinger => 104,
            BuiltinFunction::SocketSetNodelay => 105,
            BuiltinFunction::SocketSetOnlyV6 => 106,
            BuiltinFunction::SocketSetRecvSize => 107,
            BuiltinFunction::SocketSetReuseAddress => 108,
            BuiltinFunction::SocketSetReusePort => 109,
            BuiltinFunction::SocketSetSendSize => 110,
            BuiltinFunction::SocketSetTtl => 111,
            BuiltinFunction::SocketShutdownRead => 112,
            BuiltinFunction::SocketShutdownReadWrite => 113,
            BuiltinFunction::SocketShutdownWrite => 114,
            BuiltinFunction::SocketTryClone => 115,
            BuiltinFunction::SocketWriteBytes => 116,
            BuiltinFunction::SocketWriteString => 117,
            BuiltinFunction::StderrFlush => 118,
            BuiltinFunction::StderrWriteBytes => 119,
            BuiltinFunction::StderrWriteString => 120,
            BuiltinFunction::StdinRead => 121,
            BuiltinFunction::StdoutFlush => 122,
            BuiltinFunction::StdoutWriteBytes => 123,
            BuiltinFunction::StdoutWriteString => 124,
            BuiltinFunction::StringToByteArray => 125,
            BuiltinFunction::StringToFloat => 126,
            BuiltinFunction::StringToInt => 127,
            BuiltinFunction::StringToLower => 128,
            BuiltinFunction::StringToUpper => 129,
            BuiltinFunction::TimeMonotonic => 130,
            BuiltinFunction::TimeSystem => 131,
            BuiltinFunction::TimeSystemOffset => 132,
            BuiltinFunction::CpuCores => 133,
            BuiltinFunction::StringCharacters => 134,
            BuiltinFunction::StringCharactersNext => 135,
            BuiltinFunction::StringCharactersDrop => 136,
            BuiltinFunction::StringConcatArray => 137,
            BuiltinFunction::ArrayReserve => 138,
            BuiltinFunction::ArrayCapacity => 139,
            BuiltinFunction::ProcessStacktraceLength => 140,
            BuiltinFunction::FloatToBits => 141,
            BuiltinFunction::FloatFromBits => 142,
            BuiltinFunction::RandomNew => 143,
            BuiltinFunction::RandomFromInt => 144,
            BuiltinFunction::RandomDrop => 145,
            BuiltinFunction::StringSliceBytes => 146,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BuiltinFunction::ChildProcessDrop => "child_process_drop",
            BuiltinFunction::ChildProcessSpawn => "child_process_spawn",
            BuiltinFunction::ChildProcessStderrClose => {
                "child_process_stderr_close"
            }
            BuiltinFunction::ChildProcessStderrRead => {
                "child_process_stderr_read"
            }
            BuiltinFunction::ChildProcessStdinClose => {
                "child_process_stdin_close"
            }
            BuiltinFunction::ChildProcessStdinFlush => {
                "child_process_stdin_flush"
            }
            BuiltinFunction::ChildProcessStdinWriteBytes => {
                "child_process_stdin_write_bytes"
            }
            BuiltinFunction::ChildProcessStdinWriteString => {
                "child_process_stdin_write_string"
            }
            BuiltinFunction::ChildProcessStdoutClose => {
                "child_process_stdout_close"
            }
            BuiltinFunction::ChildProcessStdoutRead => {
                "child_process_stdout_read"
            }
            BuiltinFunction::ChildProcessTryWait => "child_process_try_wait",
            BuiltinFunction::ChildProcessWait => "child_process_wait",
            BuiltinFunction::EnvArguments => "env_arguments",
            BuiltinFunction::EnvExecutable => "env_executable",
            BuiltinFunction::EnvGet => "env_get",
            BuiltinFunction::EnvGetWorkingDirectory => {
                "env_get_working_directory"
            }
            BuiltinFunction::EnvHomeDirectory => "env_home_directory",
            BuiltinFunction::EnvPlatform => "env_platform",
            BuiltinFunction::EnvSetWorkingDirectory => {
                "env_set_working_directory"
            }
            BuiltinFunction::EnvTempDirectory => "env_temp_directory",
            BuiltinFunction::EnvVariables => "env_variables",
            BuiltinFunction::FFIFunctionAttach => "ffi_function_attach",
            BuiltinFunction::FFIFunctionCall => "ffi_function_call",
            BuiltinFunction::FFIFunctionDrop => "ffi_function_drop",
            BuiltinFunction::FFILibraryDrop => "ffi_library_drop",
            BuiltinFunction::FFILibraryOpen => "ffi_library_open",
            BuiltinFunction::FFIPointerAddress => "ffi_pointer_address",
            BuiltinFunction::FFIPointerAttach => "ffi_pointer_attach",
            BuiltinFunction::FFIPointerFromAddress => {
                "ffi_pointer_from_address"
            }
            BuiltinFunction::FFIPointerRead => "ffi_pointer_read",
            BuiltinFunction::FFIPointerWrite => "ffi_pointer_write",
            BuiltinFunction::FFITypeAlignment => "ffi_type_alignment",
            BuiltinFunction::FFITypeSize => "ffi_type_size",
            BuiltinFunction::DirectoryCreate => "directory_create",
            BuiltinFunction::DirectoryCreateRecursive => {
                "directory_create_recursive"
            }
            BuiltinFunction::DirectoryList => "directory_list",
            BuiltinFunction::DirectoryRemove => "directory_remove",
            BuiltinFunction::DirectoryRemoveRecursive => {
                "directory_remove_recursive"
            }
            BuiltinFunction::FileCopy => "file_copy",
            BuiltinFunction::FileDrop => "file_drop",
            BuiltinFunction::FileFlush => "file_flush",
            BuiltinFunction::FileOpenAppendOnly => "file_open_append_only",
            BuiltinFunction::FileOpenReadAppend => "file_open_read_append",
            BuiltinFunction::FileOpenReadOnly => "file_open_read_only",
            BuiltinFunction::FileOpenReadWrite => "file_open_read_write",
            BuiltinFunction::FileOpenWriteOnly => "file_open_write_only",
            BuiltinFunction::FileRead => "file_read",
            BuiltinFunction::FileRemove => "file_remove",
            BuiltinFunction::FileSeek => "file_seek",
            BuiltinFunction::FileSize => "file_size",
            BuiltinFunction::FileWriteBytes => "file_write_bytes",
            BuiltinFunction::FileWriteString => "file_write_string",
            BuiltinFunction::PathAccessedAt => "path_accessed_at",
            BuiltinFunction::PathCreatedAt => "path_created_at",
            BuiltinFunction::PathExists => "path_exists",
            BuiltinFunction::PathIsDirectory => "path_is_directory",
            BuiltinFunction::PathIsFile => "path_is_file",
            BuiltinFunction::PathModifiedAt => "path_modified_at",
            BuiltinFunction::HasherDrop => "hasher_drop",
            BuiltinFunction::HasherNew => "hasher_new",
            BuiltinFunction::HasherToHash => "hasher_to_hash",
            BuiltinFunction::HasherWriteInt => "hasher_write_int",
            BuiltinFunction::ProcessStacktraceDrop => "process_stacktrace_drop",
            BuiltinFunction::ProcessCallFrameLine => "process_call_frame_line",
            BuiltinFunction::ProcessCallFrameName => "process_call_frame_name",
            BuiltinFunction::ProcessCallFramePath => "process_call_frame_path",
            BuiltinFunction::ProcessStacktrace => "process_stacktrace",
            BuiltinFunction::RandomBytes => "random_bytes",
            BuiltinFunction::RandomFloat => "random_float",
            BuiltinFunction::RandomFloatRange => "random_float_range",
            BuiltinFunction::RandomIntRange => "random_int_range",
            BuiltinFunction::RandomInt => "random_int",
            BuiltinFunction::SocketAcceptIp => "socket_accept_ip",
            BuiltinFunction::SocketAcceptUnix => "socket_accept_unix",
            BuiltinFunction::SocketAddressPairAddress => {
                "socket_address_pair_address"
            }
            BuiltinFunction::SocketAddressPairDrop => {
                "socket_address_pair_drop"
            }
            BuiltinFunction::SocketAddressPairPort => {
                "socket_address_pair_port"
            }
            BuiltinFunction::SocketAllocateIpv4 => "socket_allocate_ipv4",
            BuiltinFunction::SocketAllocateIpv6 => "socket_allocate_ipv6",
            BuiltinFunction::SocketAllocateUnix => "socket_allocate_unix",
            BuiltinFunction::SocketBind => "socket_bind",
            BuiltinFunction::SocketConnect => "socket_connect",
            BuiltinFunction::SocketDrop => "socket_drop",
            BuiltinFunction::SocketGetBroadcast => "socket_get_broadcast",
            BuiltinFunction::SocketGetKeepalive => "socket_get_keepalive",
            BuiltinFunction::SocketGetLinger => "socket_get_linger",
            BuiltinFunction::SocketGetNodelay => "socket_get_nodelay",
            BuiltinFunction::SocketGetOnlyV6 => "socket_get_only_v6",
            BuiltinFunction::SocketGetRecvSize => "socket_get_recv_size",
            BuiltinFunction::SocketGetReuseAddress => {
                "socket_get_reuse_address"
            }
            BuiltinFunction::SocketGetReusePort => "socket_get_reuse_port",
            BuiltinFunction::SocketGetSendSize => "socket_get_send_size",
            BuiltinFunction::SocketGetTtl => "socket_get_ttl",
            BuiltinFunction::SocketListen => "socket_listen",
            BuiltinFunction::SocketLocalAddress => "socket_local_address",
            BuiltinFunction::SocketPeerAddress => "socket_peer_address",
            BuiltinFunction::SocketRead => "socket_read",
            BuiltinFunction::SocketReceiveFrom => "socket_receive_from",
            BuiltinFunction::SocketSendBytesTo => "socket_send_bytes_to",
            BuiltinFunction::SocketSendStringTo => "socket_send_string_to",
            BuiltinFunction::SocketSetBroadcast => "socket_set_broadcast",
            BuiltinFunction::SocketSetKeepalive => "socket_set_keepalive",
            BuiltinFunction::SocketSetLinger => "socket_set_linger",
            BuiltinFunction::SocketSetNodelay => "socket_set_nodelay",
            BuiltinFunction::SocketSetOnlyV6 => "socket_set_only_v6",
            BuiltinFunction::SocketSetRecvSize => "socket_set_recv_size",
            BuiltinFunction::SocketSetReuseAddress => {
                "socket_set_reuse_address"
            }
            BuiltinFunction::SocketSetReusePort => "socket_set_reuse_port",
            BuiltinFunction::SocketSetSendSize => "socket_set_send_size",
            BuiltinFunction::SocketSetTtl => "socket_set_ttl",
            BuiltinFunction::SocketShutdownRead => "socket_shutdown_read",
            BuiltinFunction::SocketShutdownReadWrite => {
                "socket_shutdown_read_write"
            }
            BuiltinFunction::SocketShutdownWrite => "socket_shutdown_write",
            BuiltinFunction::SocketTryClone => "socket_try_clone",
            BuiltinFunction::SocketWriteBytes => "socket_write_bytes",
            BuiltinFunction::SocketWriteString => "socket_write_string",
            BuiltinFunction::StderrFlush => "stderr_flush",
            BuiltinFunction::StderrWriteBytes => "stderr_write_bytes",
            BuiltinFunction::StderrWriteString => "stderr_write_string",
            BuiltinFunction::StdinRead => "stdin_read",
            BuiltinFunction::StdoutFlush => "stdout_flush",
            BuiltinFunction::StdoutWriteBytes => "stdout_write_bytes",
            BuiltinFunction::StdoutWriteString => "stdout_write_string",
            BuiltinFunction::TimeMonotonic => "time_monotonic",
            BuiltinFunction::TimeSystem => "time_system",
            BuiltinFunction::TimeSystemOffset => "time_system_offset",
            BuiltinFunction::StringToLower => "string_to_lower",
            BuiltinFunction::StringToUpper => "string_to_upper",
            BuiltinFunction::StringToByteArray => "string_to_byte_array",
            BuiltinFunction::StringToFloat => "string_to_float",
            BuiltinFunction::StringToInt => "string_to_int",
            BuiltinFunction::ByteArrayDrainToString => {
                "byte_array_drain_to_string"
            }
            BuiltinFunction::ByteArrayToString => "byte_array_to_string",
            BuiltinFunction::CpuCores => "cpu_cores",
            BuiltinFunction::StringCharacters => "string_characters",
            BuiltinFunction::StringCharactersNext => "string_characters_next",
            BuiltinFunction::StringCharactersDrop => "string_characters_drop",
            BuiltinFunction::StringConcatArray => "string_concat_array",
            BuiltinFunction::ArrayReserve => "array_reserve",
            BuiltinFunction::ArrayCapacity => "array_capacity",
            BuiltinFunction::ProcessStacktraceLength => {
                "process_stacktrace_length"
            }
            BuiltinFunction::FloatToBits => "float_to_bits",
            BuiltinFunction::FloatFromBits => "float_from_bits",
            BuiltinFunction::RandomNew => "random_new",
            BuiltinFunction::RandomFromInt => "random_from_int",
            BuiltinFunction::RandomDrop => "random_drop",
            BuiltinFunction::StringSliceBytes => "string_slice_bytes",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_opcode_from_byte() {
        assert_eq!(Opcode::from_byte(94), Ok(Opcode::Return));
        assert_eq!(
            Opcode::from_byte(255),
            Err("The opcode 255 is invalid".to_string())
        );
    }

    #[test]
    fn test_arg() {
        let ins = Instruction::new(Opcode::GetConstant, [1, 2, 0, 0, 0]);

        assert_eq!(ins.arg(0), 1);
    }

    #[test]
    fn test_u32_arg() {
        let ins = Instruction::new(Opcode::Return, [0, 14, 1, 1, 1]);

        assert_eq!(ins.u32_arg(1, 2), 65_550);
    }

    #[test]
    fn test_u64_arg() {
        let ins0 = Instruction::new(Opcode::Return, [0, 14, 1, 0, 0]);
        let ins1 = Instruction::new(Opcode::Return, [0, 14, 1, 1, 1]);

        assert_eq!(ins0.u64_arg(1, 2, 3, 4), 65_550);
        assert_eq!(ins1.u64_arg(1, 2, 3, 4), 281_479_271_743_502);
    }

    #[test]
    fn test_type_size() {
        assert_eq!(size_of::<Instruction>(), 12);
    }
}
