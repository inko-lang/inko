//! Structures for encoding virtual machine instructions.
use vm::instructions::array;
use vm::instructions::binding;
use vm::instructions::block;
use vm::instructions::boolean;
use vm::instructions::code_execution;
use vm::instructions::constant;
use vm::instructions::error;
use vm::instructions::file;
use vm::instructions::float;
use vm::instructions::control_flow;
use vm::instructions::integer;
use vm::instructions::local_variable;
use vm::instructions::method;
use vm::instructions::nil;
use vm::instructions::object;
use vm::instructions::process;
use vm::instructions::prototype;
use vm::instructions::stderr;
use vm::instructions::stdin;
use vm::instructions::stdout;
use vm::instructions::string;
use vm::machine::Machine;
use vm::instructions::result::InstructionResult;

use compiled_code::RcCompiledCode;
use process::RcProcess;

/// Enum containing all possible instruction types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    SetConst,
    GetConst,
    SetAttr,
    GetAttr,
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
    GetBindingOfCaller,
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
}

pub const INSTRUCTION_MAPPING: [fn(&Machine,
   &RcProcess,
   &RcCompiledCode,
   &Instruction)
   -> InstructionResult; 102] = [integer::set_integer,
                                 float::set_float,
                                 string::set_string,
                                 object::set_object,
                                 array::set_array,
                                 prototype::get_integer_prototype,
                                 prototype::get_float_prototype,
                                 prototype::get_string_prototype,
                                 prototype::get_array_prototype,
                                 prototype::get_true_prototype,
                                 prototype::get_false_prototype,
                                 prototype::get_method_prototype,
                                 prototype::get_block_prototype,
                                 boolean::get_true,
                                 boolean::get_false,
                                 local_variable::set_local,
                                 local_variable::get_local,
                                 block::set_block,
                                 control_flow::return_value,
                                 control_flow::goto_if_false,
                                 control_flow::goto_if_true,
                                 control_flow::goto,
                                 method::def_method,
                                 code_execution::run_block,
                                 error::is_error,
                                 integer::integer_add,
                                 integer::integer_div,
                                 integer::integer_mul,
                                 integer::integer_sub,
                                 integer::integer_mod,
                                 integer::integer_to_float,
                                 integer::integer_to_string,
                                 integer::integer_bitwise_and,
                                 integer::integer_bitwise_or,
                                 integer::integer_bitwise_xor,
                                 integer::integer_shift_left,
                                 integer::integer_shift_right,
                                 integer::integer_smaller,
                                 integer::integer_greater,
                                 integer::integer_equals,
                                 float::float_add,
                                 float::float_mul,
                                 float::float_div,
                                 float::float_sub,
                                 float::float_mod,
                                 float::float_to_integer,
                                 float::float_to_string,
                                 float::float_smaller,
                                 float::float_greater,
                                 float::float_equals,
                                 array::array_insert,
                                 array::array_at,
                                 array::array_remove,
                                 array::array_length,
                                 array::array_clear,
                                 string::string_to_lower,
                                 string::string_to_upper,
                                 string::string_equals,
                                 string::string_to_bytes,
                                 string::string_from_bytes,
                                 string::string_length,
                                 string::string_size,
                                 stdout::stdout_write,
                                 stderr::stderr_write,
                                 stdin::stdin_read,
                                 stdin::stdin_read_line,
                                 file::file_open,
                                 file::file_write,
                                 file::file_read,
                                 file::file_read_line,
                                 file::file_flush,
                                 file::file_size,
                                 file::file_seek,
                                 code_execution::parse_file,
                                 code_execution::file_parsed,
                                 prototype::get_binding_prototype,
                                 binding::get_binding,
                                 constant::set_const,
                                 constant::get_const,
                                 object::set_attr,
                                 object::get_attr,
                                 prototype::set_prototype,
                                 prototype::get_prototype,
                                 local_variable::local_exists,
                                 method::responds_to,
                                 process::spawn_process,
                                 process::send_process_message,
                                 process::receive_process_message,
                                 process::get_current_pid,
                                 local_variable::set_parent_local,
                                 local_variable::get_parent_local,
                                 binding::get_binding_of_caller,
                                 error::error_to_integer,
                                 file::file_read_exact,
                                 stdin::stdin_read_exact,
                                 object::object_equals,
                                 object::get_toplevel,
                                 prototype::get_nil_prototype,
                                 nil::get_nil,
                                 method::lookup_method,
                                 object::attr_exists,
                                 constant::const_exists];

/// Struct for storing information about a single instruction.
#[derive(Clone, Debug)]
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
    pub fn new(ins_type: InstructionType,
               arguments: Vec<u16>,
               line: u16)
               -> Instruction {
        Instruction {
            instruction_type: ins_type,
            arguments: arguments,
            line: line,
        }
    }

    pub fn arg(&self, index: usize) -> Result<usize, String> {
        self.arguments
            .get(index)
            .cloned()
            .ok_or_else(|| {
                format!("Undefined instruction argument {} for {:?}", index, self)
            })
            .map(|num| num as usize)
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
    fn test_arg_invalid() {
        let ins = new_instruction();

        assert!(ins.arg(5).is_err());
    }

    #[test]
    fn test_arg_valid() {
        let ins = new_instruction();

        assert!(ins.arg(0).is_ok());
        assert_eq!(ins.arg(0).unwrap(), 1);
    }
}
