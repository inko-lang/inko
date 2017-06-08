//! A parser for Inko bytecode streams
//!
//! This module provides various functions that can be used for parsing Inko
//! bytecode files provided as a stream of bytes.
//!
//! To parse a stream of bytes you can use the `parse` function:
//!
//!     let mut bytes = File::open("path/to/file.inkoc").unwrap().bytes();
//!     let result = bytecode_parser::parse(&mut bytes);
//!
//! Alternatively you can also parse a file directly:
//!
//!     let result = bytecode_parser::parse_file("path/to/file.inkoc");

use std::io::prelude::*;
use std::io::Bytes;
use std::fs::File;
use std::mem;

use catch_table::{CatchTable, CatchEntry, ThrowReason};
use compiled_code::CompiledCode;
use object_pointer::ObjectPointer;
use vm::instruction::{InstructionType, Instruction};
use vm::state::RcState;

macro_rules! parser_error {
    ($variant: ident) => (
        return Err(ParserError::$variant);
    );
}

macro_rules! try_byte {
    ($expr: expr, $variant: ident) => (
        match $expr {
            Some(result) => {
                match result {
                    Ok(byte) => byte,
                    Err(_)   => parser_error!($variant)
                }
            },
            None => parser_error!($variant)
        }
    );
}

macro_rules! read_u16_to_usize_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<usize, $byte_type>($bytes, read_u16_as_usize));
    );
}

macro_rules! read_instruction_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<Instruction, $byte_type>($bytes,
                                                    read_instruction));
    );
}

const SIGNATURE_BYTES: [u8; 4] = [105, 110, 107, 111]; // "inko"

const VERSION: u8 = 1;

const LITERAL_INTEGER: u8 = 0;
const LITERAL_FLOAT: u8 = 1;
const LITERAL_STRING: u8 = 2;

#[derive(Debug)]
pub enum ParserError {
    InvalidFile,
    InvalidSignature,
    InvalidVersion,
    InvalidString,
    InvalidInteger,
    InvalidFloat,
    MissingByte,
    InvalidLiteralType(u8),
    MissingReturnInstruction(String, u16),
    MissingInstructions(String, u16),
}

pub type ParserResult<T> = Result<T, ParserError>;
pub type BytecodeResult = ParserResult<CompiledCode>;

/// Parses a file
///
/// # Examples
///
///     let state = State::new(Config::new());
///     let result = bytecode_parser::parse_file(&state, "path/to/file.inkoc");
pub fn parse_file(state: &RcState, path: &str) -> BytecodeResult {
    match File::open(path) {
        Ok(file) => parse(state, &mut file.bytes()),
        Err(_) => parser_error!(InvalidFile),
    }
}

/// Parses a stream of bytes
///
/// # Examples
///
///     let mut bytes = File::open("path/to/file.inkoc").unwrap().bytes();
///     let state = State::new(Config::new());
///     let result = bytecode_parser::parse(&state, &mut bytes);
pub fn parse<T: Read>(state: &RcState, bytes: &mut Bytes<T>) -> BytecodeResult {
    // Verify the bytecode signature.
    for expected in SIGNATURE_BYTES.iter() {
        let byte = try_byte!(bytes.next(), InvalidSignature);

        if byte != *expected {
            parser_error!(InvalidSignature);
        }
    }

    // Verify the version
    if try_byte!(bytes.next(), InvalidVersion) != VERSION {
        parser_error!(InvalidVersion);
    }

    let code = try!(read_compiled_code(state, bytes));

    Ok(code)
}

fn read_string<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<String> {
    let size = try!(read_u64(bytes));

    let mut buff: Vec<u8> = Vec::new();

    for _ in 0..size {
        buff.push(try_byte!(bytes.next(), InvalidString));
    }

    match String::from_utf8(buff) {
        Ok(string) => Ok(string),
        Err(_) => parser_error!(InvalidString),
    }
}

fn read_u8<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u8> {
    let byte = try_byte!(bytes.next(), InvalidInteger);

    let value: u8 = unsafe { mem::transmute([byte]) };

    Ok(u8::from_be(value))
}

fn read_bool<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<bool> {
    let byte = try_byte!(bytes.next(), InvalidInteger);

    let value: u8 = unsafe { mem::transmute([byte]) };

    Ok(u8::from_be(value) == 1)
}

fn read_u16<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u16> {
    let mut buff: [u8; 2] = [0, 0];

    for index in 0..2 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: u16 = unsafe { mem::transmute(buff) };

    Ok(u16::from_be(value))
}

fn read_u16_as_usize<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<usize> {
    let mut buff: [u8; 2] = [0, 0];

    for index in 0..2 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: u16 = unsafe { mem::transmute(buff) };

    Ok(u16::from_be(value) as usize)
}

fn read_i64<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<i64> {
    let mut buff: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

    for index in 0..8 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: i64 = unsafe { mem::transmute(buff) };

    Ok(i64::from_be(value))
}

fn read_u64<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u64> {
    Ok(try!(read_i64(bytes)) as u64)
}

fn read_f64<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<f64> {
    let mut buff: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

    for index in 0..8 {
        buff[index] = try_byte!(bytes.next(), InvalidFloat);
    }

    let int: u64 = u64::from_be(unsafe { mem::transmute(buff) });
    let float: f64 = unsafe { mem::transmute(int) };

    Ok(float)
}

fn read_vector<V, T: Read>(bytes: &mut Bytes<T>,
                           reader: fn(&mut Bytes<T>) -> ParserResult<V>)
                           -> ParserResult<Vec<V>> {
    let amount = try!(read_u64(bytes)) as usize;
    let mut buff: Vec<V> = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(try!(reader(bytes)) as V);
    }

    Ok(buff)
}

fn read_code_vector<T: Read>(state: &RcState,
                             bytes: &mut Bytes<T>)
                             -> ParserResult<Vec<CompiledCode>> {
    let amount = try!(read_u64(bytes)) as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(try!(read_compiled_code(state, bytes)));
    }

    Ok(buff)
}

fn read_instruction<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<Instruction> {
    let ins_type: InstructionType =
        unsafe { mem::transmute(try!(read_u8(bytes))) };

    let args = read_u16_to_usize_vector!(T, bytes);
    let line = try!(read_u16(bytes));
    let ins = Instruction::new(ins_type, args, line);

    Ok(ins)
}

fn read_compiled_code<T: Read>(state: &RcState,
                               bytes: &mut Bytes<T>)
                               -> ParserResult<CompiledCode> {
    let name = try!(read_string(bytes));
    let file = try!(read_string(bytes));
    let line = try!(read_u16(bytes));
    let args = try!(read_u8(bytes));
    let req_args = try!(read_u8(bytes));
    let rest_arg = try!(read_bool(bytes));
    let locals = try!(read_u16(bytes));
    let registers = try!(read_u16(bytes));
    let captures = try!(read_bool(bytes));
    let instructions = read_instruction_vector!(T, bytes);

    // Make sure we always have a return at the end.
    if let Some(ins) = instructions.last() {
        if ins.instruction_type != InstructionType::Return {
            return Err(ParserError::MissingReturnInstruction(file, line));
        }
    } else {
        return Err(ParserError::MissingInstructions(file, line));
    }

    let literals = try!(read_literals_vector(state, bytes));
    let code_objects = try!(read_code_vector(state, bytes));
    let catch_table = try!(read_catch_table(bytes));

    Ok(CompiledCode {
        name: name,
        file: file,
        line: line,
        arguments: args,
        required_arguments: req_args,
        rest_argument: rest_arg,
        locals: locals,
        registers: registers,
        captures: captures,
        instructions: instructions,
        literals: literals,
        code_objects: code_objects,
        catch_table: catch_table,
    })
}

fn read_literals_vector<T: Read>(state: &RcState,
                                 bytes: &mut Bytes<T>)
                                 -> ParserResult<Vec<ObjectPointer>> {
    let amount = try!(read_u64(bytes));
    let mut buff = Vec::with_capacity(amount as usize);

    for _ in 0..amount {
        buff.push(try!(read_literal(state, bytes)));
    }

    Ok(buff)
}

fn read_literal<T: Read>(state: &RcState,
                         bytes: &mut Bytes<T>)
                         -> ParserResult<ObjectPointer> {
    let literal_type = try!(read_u8(bytes));

    let literal = match literal_type {
        LITERAL_INTEGER => ObjectPointer::integer(try!(read_i64(bytes))),
        LITERAL_FLOAT => state.allocate_permanent_float(try!(read_f64(bytes))),
        LITERAL_STRING => state.intern(&try!(read_string(bytes))),
        _ => return Err(ParserError::InvalidLiteralType(literal_type)),
    };

    Ok(literal)
}

fn read_catch_table<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<CatchTable> {
    let amount = try!(read_u64(bytes)) as usize;
    let mut entries = Vec::with_capacity(amount);

    for _ in 0..amount {
        entries.push(try!(read_catch_entry(bytes)));
    }

    Ok(CatchTable { entries: entries })
}

fn read_catch_entry<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<CatchEntry> {
    let reason = ThrowReason::from_u8(try!(read_u8(bytes)));
    let start = try!(read_u16_as_usize(bytes));
    let end = try!(read_u16_as_usize(bytes));
    let jump_to = try!(read_u16_as_usize(bytes));
    let register = try!(read_u16_as_usize(bytes));

    Ok(CatchEntry::new(reason, start, end, jump_to, register))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use config::Config;
    use vm::instruction::InstructionType;
    use vm::state::{State, RcState};

    fn state() -> RcState {
        State::new(Config::new())
    }

    macro_rules! unwrap {
        ($expr: expr) => ({
            match $expr {
                Ok(value)  => value,
                Err(error) => panic!("Failed to parse input: {:?}", error)
            }
        });
    }

    macro_rules! read {
        ($name: ident, $buffer: expr) => (
            $name(&mut $buffer.bytes())
        );
    }

    macro_rules! pack_u8 {
        ($num: expr, $buffer: expr) => ({
            let num   = u8::to_be($num);
            let bytes = [num];

            $buffer.extend_from_slice(&bytes);
        });
    }

    macro_rules! pack_u16 {
        ($num: expr, $buffer: expr) => ({
            let num = u16::to_be($num);
            let bytes: [u8; 2] = unsafe { mem::transmute(num) };

            $buffer.extend_from_slice(&bytes);
        });
    }

    macro_rules! pack_u64 {
        ($num: expr, $buffer: expr) => ({
            let num = u64::to_be($num);
            let bytes: [u8; 8] = unsafe { mem::transmute(num) };

            $buffer.extend_from_slice(&bytes);
        });
    }

    macro_rules! pack_f64 {
        ($num: expr, $buffer: expr) => ({
            let int: u64 = unsafe { mem::transmute($num) };

            pack_u64!(int, $buffer);
        });
    }

    macro_rules! pack_string {
        ($string: expr, $buffer: expr) => ({
            pack_u64!($string.len() as u64, $buffer);

            $buffer.extend_from_slice(&$string.as_bytes());
        });
    }

    #[test]
    fn test_parse_empty() {
        let buffer = Vec::new();
        let state = state();
        let output = parse(&state, &mut buffer.bytes());

        assert!(output.is_err());
    }

    #[test]
    fn test_parse_invalid_signature() {
        let mut buffer = Vec::new();
        let state = state();

        pack_string!("cats", buffer);

        let output = parse(&state, &mut buffer.bytes());

        assert!(output.is_err());
    }

    #[test]
    fn test_parse_invalid_version() {
        let mut buffer = Vec::new();
        let state = state();

        buffer.push(97);
        buffer.push(101);
        buffer.push(111);
        buffer.push(110);

        buffer.push(VERSION + 1);

        let output = parse(&state, &mut buffer.bytes());

        assert!(output.is_err());
    }

    #[test]
    fn test_parse() {
        let mut buffer = Vec::new();
        let state = state();

        buffer.push(105);
        buffer.push(110);
        buffer.push(107);
        buffer.push(111);

        buffer.push(VERSION);

        pack_string!("main", buffer);
        pack_string!("test.inko", buffer);
        pack_u16!(4, buffer); // line
        pack_u8!(0, buffer); // arguments
        pack_u8!(0, buffer); // required arguments
        pack_u8!(0, buffer); // rest argument
        pack_u16!(0, buffer); // locals
        pack_u16!(0, buffer); // registers
        pack_u8!(0, buffer); // captures

        pack_u64!(1, buffer); // instructions

        pack_u8!(InstructionType::Return as u8, buffer);
        pack_u64!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        pack_u64!(0, buffer); // literals
        pack_u64!(0, buffer); // code objects
        pack_u64!(0, buffer); // catch table entries

        let object = unwrap!(parse(&state, &mut buffer.bytes()));

        assert_eq!(object.name, "main".to_string());
        assert_eq!(object.file, "test.inko".to_string());
        assert_eq!(object.line, 4);
    }

    #[test]
    fn test_read_string() {
        let mut buffer = Vec::new();

        pack_string!("inko", buffer);

        let output = unwrap!(read!(read_string, buffer));

        assert_eq!(output, "inko".to_string());
    }

    #[test]
    fn test_read_string_longer_than_size() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);

        buffer.extend_from_slice(&"inko".as_bytes());

        let output = unwrap!(read!(read_string, buffer));

        assert_eq!(output, "in".to_string());
    }

    #[test]
    fn test_read_string_invalid_utf8() {
        let mut buffer = Vec::new();
        let bytes: [u8; 4] = [0, 159, 146, 150];

        pack_u64!(4, buffer);

        buffer.extend_from_slice(&bytes);

        let output = read!(read_string, buffer);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_string_empty() {
        let output = read!(read_string, []);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u8() {
        let mut buffer = Vec::new();

        pack_u8!(2, buffer);

        let output = unwrap!(read!(read_u8, buffer));

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u8_empty() {
        let output = read!(read_u8, []);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u16() {
        let mut buffer = Vec::new();

        pack_u16!(2, buffer);

        let output = unwrap!(read!(read_u16, buffer));

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u16_empty() {
        let output = read!(read_u16, []);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_i64() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);

        let output = unwrap!(read!(read_i64, buffer));

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_i64_empty() {
        let output = read!(read_i64, []);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u64() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);

        let output = unwrap!(read!(read_u64, buffer));

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_f64() {
        let mut buffer = Vec::new();

        pack_f64!(2.123456, buffer);

        let output = unwrap!(read!(read_f64, buffer));

        assert!((2.123456 - output).abs() < 0.00001);
    }

    #[test]
    fn test_read_f64_empty() {
        let output = read!(read_f64, []);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_vector() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);
        pack_string!("hello", buffer);
        pack_string!("world", buffer);

        let output = unwrap!(read_vector::<String, &[u8]>(&mut buffer.bytes(),
                                                          read_string));

        assert_eq!(output.len(), 2);
        assert_eq!(output[0], "hello".to_string());
        assert_eq!(output[1], "world".to_string());
    }

    #[test]
    fn test_read_vector_empty() {
        let buffer = Vec::new();
        let output = read_vector::<String, &[u8]>(&mut buffer.bytes(),
                                                  read_string);

        assert!(output.is_err());
    }

    #[test]
    fn test_read_instruction() {
        let mut buffer = Vec::new();

        pack_u8!(0, buffer); // type
        pack_u64!(1, buffer); // args
        pack_u16!(6, buffer);
        pack_u16!(2, buffer); // line

        let ins = unwrap!(read_instruction(&mut buffer.bytes()));

        assert_eq!(ins.instruction_type, InstructionType::SetLiteral);
        assert_eq!(ins.arguments[0], 6);
        assert_eq!(ins.line, 2);
    }

    #[test]
    fn test_read_compiled_code() {
        let mut buffer = Vec::new();
        let state = state();

        pack_string!("main", buffer); // name
        pack_string!("test.inko", buffer); // file
        pack_u16!(4, buffer); // line
        pack_u8!(3, buffer); // arguments
        pack_u8!(2, buffer); // required args
        pack_u8!(1, buffer); // rest argument
        pack_u16!(1, buffer); // locals
        pack_u16!(2, buffer); // registers
        pack_u8!(1, buffer); // captures

        // instructions
        pack_u64!(2, buffer);
        pack_u8!(InstructionType::SetLiteral as u8, buffer); // type
        pack_u64!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        pack_u8!(InstructionType::Return as u8, buffer); // type
        pack_u64!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        // literals
        pack_u64!(3, buffer);

        // integer
        pack_u8!(0, buffer);
        pack_u64!(10, buffer);

        // float
        pack_u8!(1, buffer);
        pack_f64!(1.2, buffer);

        // string
        pack_u8!(2, buffer);
        pack_string!("foo", buffer);

        // code objects
        pack_u64!(0, buffer);

        // catch table entries
        pack_u64!(1, buffer);
        pack_u8!(0, buffer); // reason
        pack_u16!(4, buffer); // start
        pack_u16!(6, buffer); // end
        pack_u16!(8, buffer); // jump-to
        pack_u16!(10, buffer); // register

        let object = unwrap!(read_compiled_code(&state, &mut buffer.bytes()));

        assert_eq!(object.name, "main".to_string());
        assert_eq!(object.file, "test.inko".to_string());
        assert_eq!(object.line, 4);
        assert_eq!(object.locals, 1);
        assert_eq!(object.registers, 2);
        assert_eq!(object.arguments, 3);
        assert_eq!(object.required_arguments, 2);
        assert_eq!(object.rest_argument, true);
        assert_eq!(object.instructions.len(), 2);
        assert!(object.captures);

        let ref ins = object.instructions[0];

        assert_eq!(ins.instruction_type, InstructionType::SetLiteral);
        assert_eq!(ins.arguments[0], 6);
        assert_eq!(ins.line, 2);

        assert_eq!(object.literals.len(), 3);

        assert!(object.literals[0] == ObjectPointer::integer(10));
        assert_eq!(object.literals[1].float_value().unwrap(), 1.2);
        assert!(object.literals[2] == state.intern(&"foo".to_string()));

        assert_eq!(object.code_objects.len(), 0);
        assert_eq!(object.catch_table.entries.len(), 1);

        let ref entry = object.catch_table.entries[0];

        assert_eq!(entry.reason, ThrowReason::Return);
        assert_eq!(entry.start, 4);
        assert_eq!(entry.end, 6);
        assert_eq!(entry.jump_to, 8);
        assert_eq!(entry.register, 10);
    }

    #[test]
    fn test_read_compiled_code_without_return() {
        let mut buffer = Vec::new();
        let state = state();

        pack_string!("main", buffer); // name
        pack_string!("test.inko", buffer); // file
        pack_u16!(4, buffer); // line
        pack_u8!(3, buffer); // arguments
        pack_u8!(2, buffer); // required args
        pack_u8!(1, buffer); // rest argument
        pack_u16!(1, buffer); // locals
        pack_u16!(2, buffer); // registers
        pack_u8!(1, buffer); // captures

        // instructions
        pack_u64!(1, buffer);
        pack_u8!(InstructionType::SetLiteral as u8, buffer); // type
        pack_u64!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        // literals
        pack_u64!(0, buffer);

        // code objects
        pack_u64!(0, buffer);

        // catch table entries
        pack_u64!(0, buffer);

        assert!(read_compiled_code(&state, &mut buffer.bytes()).is_err());
    }

    #[test]
    fn test_read_compiled_code_without_instructions() {
        let mut buffer = Vec::new();
        let state = state();

        pack_string!("main", buffer); // name
        pack_string!("test.inko", buffer); // file
        pack_u16!(4, buffer); // line
        pack_u8!(3, buffer); // arguments
        pack_u8!(2, buffer); // required args
        pack_u8!(1, buffer); // rest argument
        pack_u16!(1, buffer); // locals
        pack_u16!(2, buffer); // registers
        pack_u8!(1, buffer); // captures

        // instructions
        pack_u64!(0, buffer);

        // literals
        pack_u64!(0, buffer);

        // code objects
        pack_u64!(0, buffer);

        // catch table entries
        pack_u64!(0, buffer);

        assert!(read_compiled_code(&state, &mut buffer.bytes()).is_err());
    }
}
