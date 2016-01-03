use std::io::Bytes;
use std::io::prelude::*;
use std::mem;
use std::sync::Arc;

use bytecode_file::BytecodeFile;
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

macro_rules! read_string {
    ($bytes: expr) => (
        try!(read_string(&mut $bytes));
    );
}

macro_rules! read_isize {
    ($bytes: expr) => (
        try!(read_isize(&mut $bytes));
    );
}

macro_rules! read_usize {
    ($bytes: expr) => (
        try!(read_usize(&mut $bytes));
    );
}

macro_rules! read_string_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<String, $byte_type>(&mut $bytes, read_string));
    );
}

macro_rules! read_usize_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<usize, $byte_type>(&mut $bytes, read_usize));
    );
}

macro_rules! read_isize_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<isize, $byte_type>(&mut $bytes, read_isize));
    );
}

macro_rules! read_f64_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<f64, $byte_type>(&mut $bytes, read_f64));
    );
}

macro_rules! read_instruction_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<Instruction, $byte_type>(&mut $bytes,
                                                    read_instruction));
    );
}

macro_rules! read_code_vector {
    ($byte_type: ident, $bytes: expr) => (
        try!(read_vector::<RcCompiledCode, $byte_type>(&mut $bytes,
                                                       read_compiled_code));
    );
}

const SIGNATURE_BYTES : [u8; 4] = [97, 101, 111, 110]; // "aeon"

const VERSION      : u8 = 1;
const DEPENDENCIES : u8 = 0;
const BODY         : u8 = 1;

#[derive(Debug)]
pub enum ParserError {
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

pub fn parse<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<BytecodeFile> {
    let mut dependencies = Vec::new();

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

    // Parse the dependencies, if any.
    let mut section = try_byte!(bytes.next(), MissingByte);

    if section == DEPENDENCIES {
        let dep_count = read_usize!(bytes);

        for _ in 0..dep_count {
            dependencies.push(read_string!(bytes));
        }

        section = try_byte!(bytes.next(), MissingByte);
    }

    if section != BODY {
        parser_error!(MissingByte);
    }

    let code = try!(read_compiled_code(&mut bytes));
    let file = BytecodeFile::new(dependencies, code);

    Ok(file)
}

fn read_string<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<String> {
    let size = read_usize!(bytes);

    let mut buff: Vec<u8> = Vec::new();

    for _ in 0..size {
        buff.push(try_byte!(bytes.next(), InvalidString));
    }

    match String::from_utf8(buff) {
        Ok(string) => Ok(string),
        Err(_)     => parser_error!(InvalidString)
    }
}

#[cfg(all(target_pointer_width = "32"))]
fn read_isize<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<isize> {
    let mut buff: [u8; 4] = [0, 0, 0, 0];

    for index in 0..4 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: isize = unsafe { mem::transmute(buff) };

    Ok(isize::from_be(value))
}

#[cfg(all(target_pointer_width = "64"))]
fn read_isize<T: Read>(bytes: &mut Bytes<T>) -> ParserResult<isize> {
    let mut buff: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

    for index in 0..8 {
        buff[index] = try_byte!(bytes.next(), InvalidInteger);
    }

    let value: isize = unsafe { mem::transmute(buff) };

    Ok(isize::from_be(value))
}

fn read_usize<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<usize> {
    let int = try!(read_isize(&mut bytes));

    Ok(int as usize)
}

fn read_f64<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<f64> {
    let mut buff: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

    for index in 0..8 {
        buff[index] = try_byte!(bytes.next(), InvalidFloat);
    }

    let int: u64   = u64::from_be(unsafe { mem::transmute(buff) });
    let float: f64 = unsafe { mem::transmute(int) };

    Ok(float)
}

fn read_vector<V, T: Read>(mut bytes: &mut Bytes<T>,
                           reader: fn(&mut Bytes<T>) -> ParserResult<V>) -> ParserResult<Vec<V>> {
    let amount = read_usize!(bytes);

    let mut buff: Vec<V> = Vec::new();

    for _ in 0..amount {
        buff.push(try!(reader(&mut bytes)));
    }

    Ok(buff)
}

fn read_instruction<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<Instruction> {
    let ins_type: InstructionType = unsafe {
        mem::transmute(read_usize!(bytes) as u16)
    };

    let args   = read_usize_vector!(T, bytes);
    let line   = read_usize!(bytes);
    let column = read_usize!(bytes);
    let ins    = Instruction::new(ins_type, args, line, column);

    Ok(ins)
}

fn read_compiled_code<T: Read>(mut bytes: &mut Bytes<T>) -> ParserResult<RcCompiledCode> {
    let name     = read_string!(bytes);
    let file     = read_string!(bytes);
    let line     = read_usize!(bytes);
    let req_args = read_usize!(bytes);

    let meth_vis: MethodVisibility = unsafe {
        mem::transmute(read_usize!(bytes) as u8)
    };

    let locals         = read_string_vector!(T, bytes);
    let instructions   = read_instruction_vector!(T, bytes);
    let int_literals   = read_isize_vector!(T, bytes);
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
