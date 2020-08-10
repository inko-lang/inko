//! A parser for Inko bytecode images
//!
//! This module provides various functions that can be used for parsing Inko
//! bytecode images provided as a stream of bytes.
//!
//! Various chunks of bytecode, such as numbers, are encoded using
//! little-endian. Since most conventional CPUs use little-endian, using the
//! same endianness means we don't have to flip bits around on these CPUs.
use crate::catch_table::{CatchEntry, CatchTable};
use crate::compiled_code::CompiledCode;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;
use crate::vm::instruction::{Instruction, Opcode};
use crate::vm::state::State;
use crossbeam_channel::bounded;
use crossbeam_utils::thread::scope;
use num_bigint::BigInt;
use std::f64;
use std::fs::File;
use std::io::{BufReader, Read};
use std::mem;
use std::str;

macro_rules! read_slice {
    ($stream:expr, $amount:expr) => {{
        let mut buffer: [u8; $amount] = [0; $amount];

        $stream.read_exact(&mut buffer).map_err(|e| e.to_string())?;

        buffer
    }};
}

macro_rules! read_vec {
    ($stream:expr, $amount:expr) => {{
        let mut buffer: Vec<u8> = vec![0; $amount];

        $stream.read_exact(&mut buffer).map_err(|e| e.to_string())?;

        buffer
    }};
}

macro_rules! read_byte {
    ($stream: expr) => {{
        read_slice!($stream, 1)[0]
    }};
}

/// The bytes that every bytecode file must start with.
const SIGNATURE_BYTES: [u8; 4] = [105, 110, 107, 111]; // "inko"

/// The current version of the bytecode format.
const VERSION: u8 = 1;

/// The tag that marks the start of an integer literal.
const LITERAL_INTEGER: u8 = 0;

/// The tag that marks the start of a float literal.
const LITERAL_FLOAT: u8 = 1;

/// The tag that marks the start of a string literal.
const LITERAL_STRING: u8 = 2;

/// The tag that marks the start of a big integer literal.
const LITERAL_BIGINT: u8 = 3;

/// The number of bytes to buffer for every read from a bytecode file.
const BUFFER_SIZE: usize = 32 * 1024;

#[derive(Debug)]
pub enum ParserError {
    InvalidFile,
    InvalidSignature,
    InvalidVersion,
    InvalidString,
    InvalidByteArray,
    InvalidInteger,
    InvalidBigInteger,
    InvalidFloat,
    InvalidLiteral,
    MissingByte,
    SizeTooLarge,
    TooManyInstructionArguments,
}

pub type ParserResult<T> = Result<T, ParserError>;
pub type BytecodeResult = ParserResult<Vec<Module>>;

// TODO: parse modules in parallel if necessary

/// Parses a bytecode image stored in a file.
pub fn parse_file(state: &State, path: &str) -> Result<Vec<Module>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;

    parse(state, &mut BufReader::with_capacity(BUFFER_SIZE, file))
}

/// Parses a bytecode image as a stream of bytes.
pub fn parse(
    state: &State,
    stream: &mut dyn Read,
) -> Result<Vec<Module>, String> {
    if read_slice!(stream, 4) != SIGNATURE_BYTES {
        return Err("The bytecode signature is invalid".to_string());
    }

    let version = read_byte!(stream);

    if version != VERSION {
        return Err(format!(
            "The bytecode version {} is not supported",
            version
        ));
    }

    read_modules(state, stream)
}

fn read_modules(
    state: &State,
    stream: &mut dyn Read,
) -> Result<Vec<Module>, String> {
    let concurrency = state.config.bytecode_threads;
    let num_modules = read_u64(stream)? as usize;
    let (in_sender, in_receiver) = bounded::<Vec<u8>>(num_modules);

    scope(|s| {
        let mut modules = Vec::with_capacity(num_modules);
        let mut handles = Vec::with_capacity(concurrency);

        for _ in 0..concurrency {
            let handle = s.spawn(|_| -> Result<Vec<Module>, String> {
                let mut done = Vec::new();

                while let Ok(chunk) = in_receiver.recv() {
                    done.push(read_module(state, &mut &chunk[..])?);
                }

                Ok(done)
            });

            handles.push(handle);
        }

        for _ in 0..num_modules {
            let amount = read_u64(stream)? as usize;
            let chunk = read_vec!(stream, amount);

            in_sender
                .send(chunk)
                .expect("Failed to send a chunk of bytecode");
        }

        // We need to drop the sender before joining. If we don't, parser
        // threads won't terminate until the end of this scope. But since we are
        // joining those threads, we'd never reach that point.
        drop(in_sender);

        for handle in handles {
            modules
                .append(&mut handle.join().map_err(|e| format!("{:?}", e))??);
        }

        Ok(modules)
    })
    .map_err(|e| format!("{:?}", e))?
}

fn read_module(state: &State, stream: &mut dyn Read) -> Result<Module, String> {
    let literals = read_literals_vector(state, stream)?;
    let compiled_code = read_compiled_code(state, stream, &literals)?;
    let module = Module::new(
        compiled_code.name,
        compiled_code.file,
        compiled_code,
        literals,
    );

    Ok(module)
}

fn read_string(stream: &mut dyn Read) -> Result<String, String> {
    let size = read_u64_with_limit(stream, u32::MAX as u64)? as usize;
    let buff = read_vec!(stream, size);

    String::from_utf8(buff).map_err(|e| e.to_string())
}

fn read_byte_array(stream: &mut dyn Read) -> Result<Vec<u8>, String> {
    let size = read_u64_with_limit(stream, u32::MAX as u64)? as usize;
    let buff = read_vec!(stream, size);

    Ok(buff)
}

fn read_u8(stream: &mut dyn Read) -> Result<u8, String> {
    Ok(read_byte!(stream))
}

fn read_bool(stream: &mut dyn Read) -> Result<bool, String> {
    Ok(read_u8(stream)? == 1)
}

fn read_u16(stream: &mut dyn Read) -> Result<u16, String> {
    let buff = read_slice!(stream, 2);

    Ok(u16::from_le_bytes(buff))
}

fn read_u32(stream: &mut dyn Read) -> Result<u32, String> {
    let buff = read_slice!(stream, 4);

    Ok(u32::from_le_bytes(buff))
}

fn read_u16_as_usize(stream: &mut dyn Read) -> Result<usize, String> {
    let buff = read_slice!(stream, 2);

    Ok(u16::from_le_bytes(buff) as usize)
}

fn read_i64(stream: &mut dyn Read) -> Result<i64, String> {
    let buff = read_slice!(stream, 8);

    Ok(i64::from_le_bytes(buff))
}

fn read_u64(stream: &mut dyn Read) -> Result<u64, String> {
    let buff = read_slice!(stream, 8);

    Ok(u64::from_le_bytes(buff))
}

fn read_u64_with_limit(
    stream: &mut dyn Read,
    limit: u64,
) -> Result<u64, String> {
    let value = read_u64(stream)?;

    if value <= limit {
        Ok(value)
    } else {
        Err(format!(
            "The number {} exceeds the maximum value {}",
            value, limit
        ))
    }
}

fn read_f64(stream: &mut dyn Read) -> Result<f64, String> {
    let buff = read_slice!(stream, 8);
    let int = u64::from_le_bytes(buff);

    Ok(f64::from_bits(int))
}

fn read_code_vector(
    state: &State,
    stream: &mut dyn Read,
    literals: &[ObjectPointer],
) -> Result<Vec<CompiledCode>, String> {
    let amount = read_u64_with_limit(stream, u16::MAX as u64)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_compiled_code(state, stream, literals)?);
    }

    Ok(buff)
}

fn read_instruction(stream: &mut dyn Read) -> Result<Instruction, String> {
    let ins_type: Opcode = unsafe { mem::transmute(read_u8(stream)?) };
    let amount = read_u8(stream)? as usize;
    let mut args = [0, 0, 0, 0, 0, 0];

    if amount > 6 {
        return Err(format!(
            "Instructions are limited to 6 arguments, but {} were given",
            amount
        ));
    }

    for index in 0..amount {
        args[index] = read_u16(stream)?;
    }

    let line = read_u16(stream)?;
    let ins = Instruction::new(ins_type, args, line);

    Ok(ins)
}

fn read_instructions(
    stream: &mut dyn Read,
) -> Result<Vec<Instruction>, String> {
    let amount = read_u64_with_limit(stream, u32::MAX as u64)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_instruction(stream)?);
    }

    Ok(buff)
}

fn read_argument_names(
    stream: &mut dyn Read,
    literals: &[ObjectPointer],
) -> Result<Vec<ObjectPointer>, String> {
    let amount = read_u64_with_limit(stream, u8::MAX as u64)?;
    let mut buff = Vec::with_capacity(amount as usize);

    for _ in 0..amount {
        buff.push(read_literal_index(stream, literals)?);
    }

    Ok(buff)
}

fn read_compiled_code(
    state: &State,
    stream: &mut dyn Read,
    literals: &[ObjectPointer],
) -> Result<CompiledCode, String> {
    let name = read_literal_index(stream, literals)?;
    let file = read_literal_index(stream, literals)?;
    let line = read_u16(stream)?;
    let args = read_argument_names(stream, literals)?;
    let req_args = read_u8(stream)?;
    let locals = read_u16(stream)?;
    let registers = read_u16(stream)?;
    let captures = read_bool(stream)?;
    let instructions = read_instructions(stream)?;
    let code_objects = read_code_vector(state, stream, literals)?;
    let catch_table = read_catch_table(stream)?;

    Ok(CompiledCode {
        name,
        file,
        line,
        arguments: args,
        required_arguments: req_args,
        locals,
        registers,
        captures,
        instructions,
        code_objects,
        catch_table,
    })
}

fn read_literals_vector(
    state: &State,
    stream: &mut dyn Read,
) -> Result<Vec<ObjectPointer>, String> {
    let amount = read_u64_with_limit(stream, u32::MAX as u64)?;
    let mut buff = Vec::with_capacity(amount as usize);

    for _ in 0..amount {
        buff.push(read_literal(state, stream)?);
    }

    Ok(buff)
}

fn read_literal(
    state: &State,
    stream: &mut dyn Read,
) -> Result<ObjectPointer, String> {
    let literal_type = read_u8(stream)?;

    let literal = match literal_type {
        LITERAL_INTEGER => {
            let num = read_i64(stream)?;

            if ObjectPointer::integer_too_large(num) {
                state.allocate_permanent_integer(num)
            } else {
                ObjectPointer::integer(num)
            }
        }
        LITERAL_BIGINT => {
            let bytes = read_byte_array(stream)?;

            let bigint = if let Some(bigint) = BigInt::parse_bytes(&bytes, 16) {
                bigint
            } else {
                return Err(format!(
                    "The bytes {:?} could not be parsed as a big integer",
                    bytes
                ));
            };

            state.allocate_permanent_bigint(bigint)
        }
        LITERAL_FLOAT => state.allocate_permanent_float(read_f64(stream)?),
        LITERAL_STRING => state.intern_string(read_string(stream)?),
        _ => {
            return Err(format!("The literal type {} is invalid", literal_type))
        }
    };

    Ok(literal)
}

fn read_literal_index(
    stream: &mut dyn Read,
    literals: &[ObjectPointer],
) -> Result<ObjectPointer, String> {
    let index = read_u32(stream)? as usize;

    if let Some(ptr) = literals.get(index) {
        Ok(*ptr)
    } else {
        Err(format!("The literal index {} is invalid", index))
    }
}

fn read_catch_table(stream: &mut dyn Read) -> Result<CatchTable, String> {
    let amount = read_u64_with_limit(stream, u16::MAX as u64)? as usize;
    let mut entries = Vec::with_capacity(amount);

    for _ in 0..amount {
        entries.push(read_catch_entry(stream)?);
    }

    Ok(CatchTable { entries })
}

fn read_catch_entry(stream: &mut dyn Read) -> Result<CatchEntry, String> {
    let start = read_u16_as_usize(stream)?;
    let end = read_u16_as_usize(stream)?;
    let jump_to = read_u16_as_usize(stream)?;

    Ok(CatchEntry::new(start, end, jump_to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::vm::instruction::Opcode;
    use crate::vm::state::{RcState, State};
    use std::mem;
    use std::u64;

    fn state() -> RcState {
        State::with_rc(Config::new(), &[])
    }

    macro_rules! unwrap {
        ($expr:expr) => {{
            match $expr {
                Ok(value) => value,
                Err(error) => panic!("Failed to parse input: {:?}", error),
            }
        }};
    }

    macro_rules! read {
        ($name:ident, $buffer:expr) => {
            $name(&mut BufReader::new($buffer.as_slice()))
        };
    }

    macro_rules! pack_u8 {
        ($num:expr, $buffer:expr) => {{
            let num = u8::to_le($num);
            let bytes = [num];

            $buffer.extend_from_slice(&bytes);
        }};
    }

    macro_rules! pack_u16 {
        ($num:expr, $buffer:expr) => {{
            let num = u16::to_le($num);
            let bytes: [u8; 2] = unsafe { mem::transmute(num) };

            $buffer.extend_from_slice(&bytes);
        }};
    }

    macro_rules! pack_u32 {
        ($num:expr, $buffer:expr) => {{
            let num = u32::to_le($num);
            let bytes: [u8; 4] = unsafe { mem::transmute(num) };

            $buffer.extend_from_slice(&bytes);
        }};
    }

    macro_rules! pack_u64 {
        ($num:expr, $buffer:expr) => {{
            let num = u64::to_le($num);
            let bytes: [u8; 8] = unsafe { mem::transmute(num) };

            $buffer.extend_from_slice(&bytes);
        }};
    }

    macro_rules! pack_f64 {
        ($num:expr, $buffer:expr) => {{
            let int: u64 = unsafe { mem::transmute($num) };

            pack_u64!(int, $buffer);
        }};
    }

    macro_rules! pack_string {
        ($string:expr, $buffer:expr) => {{
            pack_u64!($string.len() as u64, $buffer);

            $buffer.extend_from_slice(&$string.as_bytes());
        }};
    }

    #[test]
    fn test_parse_empty() {
        let buffer = Vec::new();
        let state = state();
        let output = parse(&state, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_parse_invalid_signature() {
        let mut buffer = Vec::new();
        let state = state();

        pack_string!("cats", buffer);

        let output = parse(&state, &mut BufReader::new(buffer.as_slice()));

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

        let output = parse(&state, &mut BufReader::new(buffer.as_slice()));

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

        pack_u64!(1, buffer); // 1 module
        pack_u64!(93, buffer); // number of bytes beyond this point
        pack_u64!(2, buffer); // literals

        pack_u8!(LITERAL_STRING, buffer);
        pack_string!("main", buffer);

        pack_u8!(LITERAL_STRING, buffer);
        pack_string!("test.inko", buffer);

        pack_u32!(0, buffer); // name
        pack_u32!(1, buffer); // file
        pack_u16!(4, buffer); // line
        pack_u64!(0, buffer); // arguments
        pack_u8!(0, buffer); // required arguments
        pack_u16!(0, buffer); // locals
        pack_u16!(0, buffer); // registers
        pack_u8!(0, buffer); // captures

        pack_u64!(1, buffer); // instructions

        pack_u8!(Opcode::Return as u8, buffer);
        pack_u8!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        pack_u64!(0, buffer); // code objects
        pack_u64!(0, buffer); // catch table entries

        let modules =
            unwrap!(parse(&state, &mut BufReader::new(buffer.as_slice())));

        let module = &modules[0];

        assert_eq!(module.name().string_value().unwrap().as_slice(), "main");
        assert_eq!(
            module.path().string_value().unwrap().as_slice(),
            "test.inko"
        );
    }

    #[test]
    fn test_read_string() {
        let mut buffer = Vec::new();

        pack_string!("inko", buffer);

        let output = unwrap!(read!(read_string, buffer));

        assert_eq!(output, "inko".to_string());
    }

    #[test]
    fn test_read_string_too_large() {
        let mut buffer = Vec::new();

        pack_u64!(u64::MAX, buffer);

        let output = read_string(&mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_byte_array() {
        let mut buffer = Vec::new();

        pack_string!("inko", buffer);

        let output = unwrap!(read!(read_byte_array, buffer));

        assert_eq!(output, vec![105, 110, 107, 111]);
    }

    #[test]
    fn test_read_byte_array_too_large() {
        let mut buffer = Vec::new();

        pack_u64!(u64::MAX, buffer);

        let output = read_byte_array(&mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
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
        let output = read!(read_string, Vec::new());

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
        let output = read!(read_u8, Vec::new());

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
        let output = read!(read_u16, Vec::new());

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
        let output = read!(read_i64, Vec::new());

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u64_with_limit() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);

        let output =
            read_u64_with_limit(&mut BufReader::new(buffer.as_slice()), 2);

        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 2);
    }

    #[test]
    fn test_read_u64_with_limit_exceeded() {
        let mut buffer = Vec::new();

        pack_u64!(2, buffer);

        let output =
            read_u64_with_limit(&mut BufReader::new(buffer.as_slice()), 1);

        assert!(output.is_err());
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
        let output = read!(read_f64, Vec::new());

        assert!(output.is_err());
    }

    #[test]
    fn test_read_instruction() {
        let mut buffer = Vec::new();

        pack_u8!(0, buffer); // type
        pack_u8!(1, buffer); // args
        pack_u16!(6, buffer);
        pack_u16!(2, buffer); // line

        let ins =
            unwrap!(read_instruction(&mut BufReader::new(buffer.as_slice())));

        assert_eq!(ins.opcode, Opcode::SetLiteral);
        assert_eq!(ins.arg(0), 6);
        assert_eq!(ins.line, 2);
    }

    #[test]
    fn test_read_instructions() {
        let mut buffer = Vec::new();

        pack_u64!(1, buffer);
        pack_u8!(0, buffer); // type
        pack_u8!(1, buffer); // args
        pack_u16!(6, buffer);
        pack_u16!(2, buffer); // line

        let instructions =
            unwrap!(read_instructions(&mut BufReader::new(buffer.as_slice())));

        assert_eq!(instructions.len(), 1);

        let ins = &instructions[0];

        assert_eq!(ins.opcode, Opcode::SetLiteral);
        assert_eq!(ins.arg(0), 6);
        assert_eq!(ins.line, 2);
    }

    #[test]
    fn test_read_compiled_code() {
        let mut buffer = Vec::new();
        let state = state();
        let literals = vec![
            state.intern_string("main".to_string()),
            state.intern_string("test.inko".to_string()),
            state.intern_string("foo".to_string()),
            state.intern_string("bar".to_string()),
            state.intern_string("baz".to_string()),
        ];

        pack_u32!(0, buffer); // name
        pack_u32!(1, buffer); // file
        pack_u16!(4, buffer); // line

        pack_u64!(3, buffer); // arguments
        pack_u32!(2, buffer); // foo
        pack_u32!(3, buffer); // bar
        pack_u32!(4, buffer); // baz

        pack_u8!(2, buffer); // required args
        pack_u16!(1, buffer); // locals
        pack_u16!(2, buffer); // registers
        pack_u8!(1, buffer); // captures

        // instructions
        pack_u64!(2, buffer);
        pack_u8!(Opcode::SetLiteral as u8, buffer); // type
        pack_u8!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        pack_u8!(Opcode::Return as u8, buffer); // type
        pack_u8!(1, buffer); // args count
        pack_u16!(6, buffer); // arg 1
        pack_u16!(2, buffer); // line number

        // code objects
        pack_u64!(0, buffer);

        // catch table entries
        pack_u64!(1, buffer);
        pack_u16!(4, buffer); // start
        pack_u16!(6, buffer); // end
        pack_u16!(8, buffer); // jump-to
        pack_u16!(10, buffer); // register

        let object = unwrap!(read_compiled_code(
            &state,
            &mut BufReader::new(buffer.as_slice()),
            &literals
        ));

        assert_eq!(object.name.string_value().unwrap().as_slice(), "main");
        assert_eq!(object.file.string_value().unwrap().as_slice(), "test.inko");
        assert_eq!(object.line, 4);
        assert_eq!(object.locals, 1);
        assert_eq!(object.registers, 2);
        assert_eq!(object.arguments.len(), 3);
        assert_eq!(object.required_arguments, 2);
        assert_eq!(object.instructions.len(), 2);
        assert!(object.captures);

        let ref ins = object.instructions[0];

        assert_eq!(ins.opcode, Opcode::SetLiteral);
        assert_eq!(ins.arg(0), 6);
        assert_eq!(ins.line, 2);

        assert_eq!(object.code_objects.len(), 0);
        assert_eq!(object.catch_table.entries.len(), 1);

        let ref entry = object.catch_table.entries[0];

        assert_eq!(entry.start, 4);
        assert_eq!(entry.end, 6);
        assert_eq!(entry.jump_to, 8);
    }
}
