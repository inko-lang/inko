use std::io::prelude::*;
use std::io::Bytes;
use std::fs::File;
use std::mem;
use std::sync::Arc;

use compiled_code::{MethodVisibility, CompiledCode, RcCompiledCode};
use instruction::{InstructionType, Instruction};

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

macro_rules! read_string_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<String, $byte_type>($bytes, read_string));
    );
}

macro_rules! read_u32_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<u32, $byte_type>($bytes, read_u32));
    );
}

macro_rules! read_i64_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<i64, $byte_type>($bytes, read_i64));
    );
}

macro_rules! read_f64_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<f64, $byte_type>($bytes, read_f64));
    );
}

macro_rules! read_instruction_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<Instruction, $byte_type>($bytes,
                                                    read_instruction));
    );
}

macro_rules! read_code_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<RcCompiledCode, $byte_type>($bytes,
                                                       read_compiled_code));
    );
}

const SIGNATURE_BYTES : [u8; 4] = [97, 101, 111, 110]; // "aeon"

const VERSION: u8 = 1;

#[derive(Debug)]
pub enum ParserError {
    InvalidFile,
    InvalidSignature,
    InvalidVersion,
    InvalidDependencies,
    InvalidString,
    InvalidInteger,
    InvalidFloat,
    InvalidVector,
    MissingByte
}

pub type ParserResult<T> = Result<T, ParserError>;
pub type BytecodeResult  = ParserResult<RcCompiledCode>;

pub fn parse_file(path: &String) -> BytecodeResult {
    match File::open(path) {
        Ok(file) => parse(&mut file.bytes()),
        Err(_)   => parser_error!(InvalidFile)
    }
}

pub fn parse<T: Read>(bytes: &mut Bytes<T>) -> BytecodeResult {
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

    let code = try!(read_compiled_code(bytes));

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
        Err(_)     => parser_error!(InvalidString)
    }
}

fn read_u8<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u8> {
    let byte = try_byte!(bytes.next(), InvalidInteger);

    let value: u8 = unsafe { mem::transmute([byte]) };

    Ok(u8::from_be(value))
}

fn read_u16<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u16> {
    let mut buff: [u8; 2] = [0, 0];

    for index in 0..2 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: u16 = unsafe { mem::transmute(buff) };

    Ok(u16::from_be(value))
}

fn read_i32<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<i32> {
    let mut buff: [u8; 4] = [0, 0, 0, 0];

    for index in 0..4 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: i32 = unsafe { mem::transmute(buff) };

    Ok(i32::from_be(value))
}

fn read_u32<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<u32> {
    Ok(try!(read_i32(bytes)) as u32)
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

    let int: u64   = u64::from_be(unsafe { mem::transmute(buff) });
    let float: f64 = unsafe { mem::transmute(int) };

    Ok(float)
}

fn read_vector<V, T: Read>(bytes: &mut Bytes<T>,
                           reader: fn(&mut Bytes<T>) -> ParserResult<V>) -> ParserResult<Vec<V>> {
    let amount = try!(read_u64(bytes));

    let mut buff: Vec<V> = Vec::new();

    for _ in 0..amount {
        buff.push(try!(reader(bytes)));
    }

    Ok(buff)
}

fn read_instruction<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<Instruction> {
    let ins_type: InstructionType = unsafe {
        mem::transmute(try!(read_u16(bytes)))
    };

    let args   = read_u32_vector!(T, bytes);
    let line   = try!(read_u32(bytes));
    let column = try!(read_u32(bytes));
    let ins    = Instruction::new(ins_type, args, line, column);

    Ok(ins)
}

fn read_compiled_code<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<RcCompiledCode> {
    let name     = try!(read_string(bytes));
    let file     = try!(read_string(bytes));
    let line     = try!(read_u32(bytes));
    let req_args = try!(read_u32(bytes));

    let meth_vis: MethodVisibility = unsafe {
        mem::transmute(try!(read_u8(bytes)))
    };

    let locals         = read_string_vector!(T, bytes);
    let instructions   = read_instruction_vector!(T, bytes);
    let int_literals   = read_i64_vector!(T, bytes);
    let float_literals = read_f64_vector!(T, bytes);
    let str_literals   = read_string_vector!(T, bytes);
    let code_objects   = read_code_vector!(T, bytes);

    let code_obj = CompiledCode {
        name: name,
        file: file,
        line: line,
        required_arguments: req_args,
        visibility: meth_vis,
        locals: locals,
        instructions: instructions,
        integer_literals: int_literals,
        float_literals: float_literals,
        string_literals: str_literals,
        code_objects: code_objects
    };

    Ok(Arc::new(code_obj))
}
