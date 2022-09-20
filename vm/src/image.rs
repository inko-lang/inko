//! Loading of Inko bytecode images.
//!
//! Various chunks of bytecode, such as numbers, are encoded using
//! little-endian. Since most conventional CPUs use little-endian, using the
//! same endianness means we don't have to flip bits around on these CPUs.
use crate::chunk::Chunk;
use crate::config::Config;
use crate::indexes::{ClassIndex, MethodIndex};
use crate::location_table::LocationTable;
use crate::mem::{Class, ClassPointer, Method, Module, ModulePointer, Pointer};
use crate::permanent_space::{
    MethodCounts, PermanentSpace, ARRAY_CLASS, BOOLEAN_CLASS, BYTE_ARRAY_CLASS,
    FLOAT_CLASS, FUTURE_CLASS, INT_CLASS, NIL_CLASS, STRING_CLASS,
};
use bytecode::{
    Instruction, Opcode, CONST_FLOAT, CONST_INTEGER, CONST_STRING,
    SIGNATURE_BYTES, VERSION,
};
use std::f64;
use std::fs::File;
use std::io::{BufReader, Read};
use std::str;

macro_rules! read_slice {
    ($stream:expr, $amount:expr) => {{
        let mut buffer: [u8; $amount] = [0; $amount];

        $stream
            .read_exact(&mut buffer)
            .map_err(|e| format!("Failed to read {} bytes: {}", $amount, e))?;

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

/// The number of bytes to buffer for every read from a bytecode file.
const BUFFER_SIZE: usize = 32 * 1024;

/// A parsed bytecode image.
pub struct Image {
    /// The ID of the first class to run.
    pub(crate) entry_class: ClassPointer,

    /// The index of the entry module to run.
    pub(crate) entry_method: MethodIndex,

    /// The space to use for allocating permanent objects.
    pub(crate) permanent_space: PermanentSpace,

    /// Configuration settings to use when parsing and running bytecode.
    pub(crate) config: Config,
}

impl Image {
    /// Loads a bytecode image from a file.
    pub fn load_file(config: Config, path: &str) -> Result<Image, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;

        Self::load(config, &mut BufReader::with_capacity(BUFFER_SIZE, file))
    }

    pub fn load_bytes(config: Config, bytes: Vec<u8>) -> Result<Image, String> {
        Self::load(
            config,
            &mut BufReader::with_capacity(BUFFER_SIZE, bytes.as_slice()),
        )
    }

    /// Loads a bytecode image from a stream of bytes.
    fn load<R: Read>(config: Config, stream: &mut R) -> Result<Image, String> {
        // This is a simply/naive check to make sure we're _probably_ loading an
        // Inko bytecode image; instead of something random like a PNG file.
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

        let modules = read_u32(stream)?;
        let classes = read_u32(stream)?;
        let space = PermanentSpace::new(
            modules,
            classes,
            read_builtin_method_counts(stream)?,
        );

        let entry_class = ClassIndex::new(read_u32(stream)?);
        let entry_method = read_u16(stream)?;

        // Now we can load the bytecode for all modules, including the entry
        // module. The order in which the modules are returned is unspecified.
        read_modules(modules as usize, &space, stream)?;

        Ok(Image {
            entry_class: unsafe { space.get_class(entry_class) },
            entry_method: MethodIndex::new(entry_method),
            permanent_space: space,
            config,
        })
    }
}

fn read_builtin_method_counts<R: Read>(
    stream: &mut R,
) -> Result<MethodCounts, String> {
    let int_class = read_u16(stream)?;
    let float_class = read_u16(stream)?;
    let string_class = read_u16(stream)?;
    let array_class = read_u16(stream)?;
    let boolean_class = read_u16(stream)?;
    let nil_class = read_u16(stream)?;
    let byte_array_class = read_u16(stream)?;
    let future_class = read_u16(stream)?;
    let counts = MethodCounts {
        int_class,
        float_class,
        string_class,
        array_class,
        boolean_class,
        nil_class,
        byte_array_class,
        future_class,
    };

    Ok(counts)
}

fn read_modules<R: Read>(
    num_modules: usize,
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<(), String> {
    for _ in 0..num_modules {
        let amount = read_u64(stream)? as usize;
        let chunk = read_vec!(stream, amount);

        read_module(space, &mut &chunk[..])?;
    }

    Ok(())
}

fn read_module<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<ModulePointer, String> {
    let index = read_u32(stream)?;
    let constants = read_constants(space, stream)?;
    let class_index = read_u32(stream)?;

    read_classes(space, stream, &constants)?;

    let mod_class = unsafe { space.get_class(ClassIndex::new(class_index)) };
    let module = Module::alloc(mod_class);

    unsafe { space.add_module(index, module)? };

    Ok(module)
}

fn read_string<R: Read>(stream: &mut R) -> Result<String, String> {
    let size = read_u32(stream)? as usize;
    let buff = read_vec!(stream, size);

    String::from_utf8(buff).map_err(|e| e.to_string())
}

fn read_u8<R: Read>(stream: &mut R) -> Result<u8, String> {
    Ok(read_byte!(stream))
}

fn read_u16<R: Read>(stream: &mut R) -> Result<u16, String> {
    let buff = read_slice!(stream, 2);

    Ok(u16::from_le_bytes(buff))
}

fn read_u32<R: Read>(stream: &mut R) -> Result<u32, String> {
    let buff = read_slice!(stream, 4);

    Ok(u32::from_le_bytes(buff))
}

fn read_i64<R: Read>(stream: &mut R) -> Result<i64, String> {
    let buff = read_slice!(stream, 8);

    Ok(i64::from_le_bytes(buff))
}

fn read_u64<R: Read>(stream: &mut R) -> Result<u64, String> {
    let buff = read_slice!(stream, 8);

    Ok(u64::from_le_bytes(buff))
}

fn read_f64<R: Read>(stream: &mut R) -> Result<f64, String> {
    let buff = read_slice!(stream, 8);
    let int = u64::from_le_bytes(buff);

    Ok(f64::from_bits(int))
}

fn read_instruction<R: Read>(
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<Instruction, String> {
    let opcode = Opcode::from_byte(read_u8(stream)?)?;
    let mut args = [0, 0, 0, 0, 0];

    for index in 0..opcode.arity() {
        args[index] = read_u16(stream)?;
    }

    let mut ins = Instruction::new(opcode, args);

    // GetConstant instructions are rewritten such that the pointer to the value
    // is encoded directly into its arguments.
    if let Opcode::GetConstant = opcode {
        let ptr = unsafe { *constants.get(ins.u32_arg(1, 2) as usize) };
        let addr = ptr.as_ptr() as u64;
        let bytes = u64::to_le_bytes(addr);

        ins.arguments[1] = u16::from_le_bytes([bytes[0], bytes[1]]);
        ins.arguments[2] = u16::from_le_bytes([bytes[2], bytes[3]]);
        ins.arguments[3] = u16::from_le_bytes([bytes[4], bytes[5]]);
        ins.arguments[4] = u16::from_le_bytes([bytes[6], bytes[7]]);
    }

    Ok(ins)
}

fn read_instructions<R: Read>(
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<Vec<Instruction>, String> {
    let amount = read_u32(stream)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_instruction(stream, constants)?);
    }

    Ok(buff)
}

fn read_classes<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<(), String> {
    let amount = read_u16(stream)?;

    for _ in 0..amount {
        read_class(space, stream, constants)?;
    }

    Ok(())
}

fn read_class<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<(), String> {
    let index = read_u32(stream)?;
    let process_class = read_u8(stream)? == 1;
    let name = read_string(stream)?;
    let fields = read_u8(stream)? as usize;
    let method_slots = read_u16(stream)?;
    let class = match index as usize {
        INT_CLASS => space.int_class(),
        FLOAT_CLASS => space.float_class(),
        STRING_CLASS => space.string_class(),
        ARRAY_CLASS => space.array_class(),
        BOOLEAN_CLASS => space.boolean_class(),
        NIL_CLASS => space.nil_class(),
        BYTE_ARRAY_CLASS => space.byte_array_class(),
        FUTURE_CLASS => space.future_class(),
        _ => {
            let new_class = if process_class {
                Class::process(name, fields, method_slots)
            } else {
                Class::object(name, fields, method_slots)
            };

            unsafe { space.add_class(index, new_class)? };
            new_class
        }
    };

    read_methods(stream, class, constants)?;
    Ok(())
}

fn read_method<R: Read>(
    stream: &mut R,
    class: ClassPointer,
    constants: &Chunk<Pointer>,
) -> Result<(), String> {
    let index = read_u16(stream)?;
    let hash = read_u32(stream)?;
    let registers = read_u16(stream)?;
    let instructions = read_instructions(stream, constants)?;
    let locations = read_location_table(stream, constants)?;
    let jump_tables = read_jump_tables(stream)?;
    let method =
        Method::alloc(hash, registers, instructions, locations, jump_tables);

    unsafe {
        class.set_method(MethodIndex::new(index), method);
    }

    Ok(())
}

fn read_location_table<R: Read>(
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<LocationTable, String> {
    let mut table = LocationTable::new();

    for _ in 0..read_u16(stream)? {
        let index = read_u32(stream)?;
        let line = read_u16(stream)?;
        let file = read_constant_index(stream, constants)?;
        let name = read_constant_index(stream, constants)?;

        table.add_entry(index, line, file, name);
    }

    Ok(table)
}

fn read_jump_tables<R: Read>(
    stream: &mut R,
) -> Result<Vec<Vec<usize>>, String> {
    let num_tables = read_u16(stream)? as usize;
    let mut tables = Vec::with_capacity(num_tables);

    for _ in 0..num_tables {
        let num_entries = read_u16(stream)? as usize;
        let mut table = Vec::with_capacity(num_entries);

        for _ in 0..num_entries {
            table.push(read_u32(stream)? as usize);
        }

        tables.push(table);
    }

    Ok(tables)
}

fn read_methods<R: Read>(
    stream: &mut R,
    class: ClassPointer,
    constants: &Chunk<Pointer>,
) -> Result<(), String> {
    let amount = read_u16(stream)? as usize;

    for _ in 0..amount {
        read_method(stream, class, constants)?;
    }

    Ok(())
}

fn read_constants<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<Chunk<Pointer>, String> {
    let amount = read_u32(stream)? as usize;
    let mut buff = Chunk::new(amount);

    for _ in 0..amount {
        let index = read_u32(stream)? as usize;

        unsafe {
            buff.set(index, read_constant(space, stream)?);
        }
    }

    Ok(buff)
}

fn read_constant_index<R: Read>(
    stream: &mut R,
    constants: &Chunk<Pointer>,
) -> Result<Pointer, String> {
    let index = read_u32(stream)? as usize;

    if index >= constants.len() {
        return Err(format!(
            "The constant index {} is out of bounds (number of constants: {})",
            index,
            constants.len()
        ));
    }

    Ok(unsafe { *constants.get(index) })
}

fn read_constant<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<Pointer, String> {
    let const_type = read_u8(stream)?;
    let value = match const_type {
        CONST_INTEGER => space.allocate_int(read_i64(stream)?),
        CONST_FLOAT => space.allocate_float(read_f64(stream)?),
        CONST_STRING => space.allocate_string(read_string(stream)?),
        _ => {
            return Err(format!("The constant type {} is invalid", const_type))
        }
    };

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::{ClassIndex, MethodIndex, ModuleIndex};
    use crate::mem::{Float, Int, String as InkoString};
    use crate::test::OwnedClass;
    use bytecode::Opcode;
    use std::u64;

    fn pack_signature(buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&SIGNATURE_BYTES);
    }

    fn pack_u8(buffer: &mut Vec<u8>, value: u8) {
        buffer.push(value);
    }

    fn pack_u16(buffer: &mut Vec<u8>, value: u16) {
        let num = u16::to_le(value);
        let bytes = num.to_ne_bytes();

        buffer.extend_from_slice(&bytes);
    }

    fn pack_u32(buffer: &mut Vec<u8>, value: u32) {
        let num = u32::to_le(value);
        let bytes = num.to_ne_bytes();

        buffer.extend_from_slice(&bytes);
    }

    fn pack_u64(buffer: &mut Vec<u8>, value: u64) {
        let num = u64::to_le(value);
        let bytes = num.to_ne_bytes();

        buffer.extend_from_slice(&bytes);
    }

    fn pack_i64(buffer: &mut Vec<u8>, value: i64) {
        let num = i64::to_le(value);
        let bytes = num.to_ne_bytes();

        buffer.extend_from_slice(&bytes);
    }

    fn pack_f64(buffer: &mut Vec<u8>, value: f64) {
        pack_u64(buffer, value.to_bits());
    }

    fn pack_string(buffer: &mut Vec<u8>, string: &str) {
        pack_u32(buffer, string.len() as u32);

        buffer.extend_from_slice(string.as_bytes());
    }

    fn pack_version(buffer: &mut Vec<u8>) {
        buffer.push(VERSION);
    }

    macro_rules! reader {
        ($buff: expr) => {
            &mut $buff.as_slice()
        };
    }

    #[test]
    fn test_load_empty() {
        let config = Config::new();
        let buffer = Vec::new();
        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_invalid_signature() {
        let config = Config::new();
        let mut buffer = Vec::new();

        pack_string(&mut buffer, "cats");

        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_invalid_version() {
        let config = Config::new();
        let mut buffer = Vec::new();

        pack_signature(&mut buffer);

        buffer.push(VERSION + 1);

        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_valid() {
        let config = Config::new();
        let mut image = Vec::new();
        let mut chunk = Vec::new();

        pack_u32(&mut chunk, 0); // module index

        // The module's constants
        pack_u32(&mut chunk, 3);

        pack_u32(&mut chunk, 0);
        pack_u8(&mut chunk, CONST_STRING);
        pack_string(&mut chunk, "new_counter");

        pack_u32(&mut chunk, 1);
        pack_u8(&mut chunk, CONST_STRING);
        pack_string(&mut chunk, "main.inko");

        pack_u32(&mut chunk, 2);
        pack_u8(&mut chunk, CONST_STRING);
        pack_string(&mut chunk, "add");

        // The (global) index of the module's class.
        pack_u32(&mut chunk, 8);

        // Classes defined in the module
        pack_u16(&mut chunk, 2); // class count

        pack_u32(&mut chunk, 8); // index
        pack_u8(&mut chunk, 0);
        pack_string(&mut chunk, "main");
        pack_u8(&mut chunk, 0);
        pack_u16(&mut chunk, 1); // Method slot count

        // The methods
        pack_u16(&mut chunk, 1);
        pack_u16(&mut chunk, 0);
        pack_u32(&mut chunk, 456);
        pack_u16(&mut chunk, 0);

        // The method instructions
        pack_u32(&mut chunk, 1);
        pack_u8(&mut chunk, 94);
        pack_u16(&mut chunk, 2);

        // The location table
        pack_u16(&mut chunk, 0);

        // The jump tables
        pack_u16(&mut chunk, 0);

        pack_u32(&mut chunk, 9); // index
        pack_u8(&mut chunk, 0);
        pack_string(&mut chunk, "Counter");
        pack_u8(&mut chunk, 1);
        pack_u16(&mut chunk, 1); // Method slot count

        // The methods of the class
        pack_u16(&mut chunk, 1);
        pack_u16(&mut chunk, 0);
        pack_u32(&mut chunk, 123);
        pack_u16(&mut chunk, 2);

        // The method instructions
        pack_u32(&mut chunk, 1);
        pack_u8(&mut chunk, 94);
        pack_u16(&mut chunk, 2);

        // The location table
        pack_u16(&mut chunk, 0);

        // The jump tables
        pack_u16(&mut chunk, 0);

        // Image header
        pack_signature(&mut image);
        pack_version(&mut image);

        pack_u32(&mut image, 1); // Number of modules
        pack_u32(&mut image, 2); // Number of classes

        // Built-in method counts
        pack_u16(&mut image, 1); // Int
        pack_u16(&mut image, 4); // Float
        pack_u16(&mut image, 5); // String
        pack_u16(&mut image, 6); // Array
        pack_u16(&mut image, 9); // Bool
        pack_u16(&mut image, 10); // NilType
        pack_u16(&mut image, 11); // ByteArray
        pack_u16(&mut image, 13); // Future

        // Entry class and method
        pack_u32(&mut image, 0);
        pack_u16(&mut image, 42);

        // The number of bytes for this module.
        pack_u64(&mut image, chunk.len() as u64);
        image.append(&mut chunk);

        let image =
            Image::load(config, &mut BufReader::new(image.as_slice())).unwrap();

        let entry_method: u16 = image.entry_method.into();
        let perm = &image.permanent_space;

        assert_eq!(perm.int_class().method_slots, 1);
        assert_eq!(perm.float_class().method_slots, 4);
        assert_eq!(perm.string_class().method_slots, 5);
        assert_eq!(perm.array_class().method_slots, 6);
        assert_eq!(perm.boolean_class().method_slots, 9);
        assert_eq!(perm.nil_class().method_slots, 10);
        assert_eq!(perm.byte_array_class().method_slots, 11);
        assert_eq!(perm.future_class().method_slots, 13);

        assert!(
            image.entry_class == unsafe { perm.get_class(ClassIndex::new(0)) }
        );
        assert_eq!(entry_method, 42);

        let module = unsafe { perm.get_module(ModuleIndex::new(0)) };

        assert_eq!(module.name(), &"main");

        let module_class = unsafe { perm.get_class(ClassIndex::new(8)) };
        let counter_class = unsafe { perm.get_class(ClassIndex::new(9)) };

        assert_eq!(&module_class.name, &"main");
        assert_eq!(module_class.method_slots, 1);

        assert_eq!(&counter_class.name, &"Counter");
        assert_eq!(counter_class.method_slots, 1);

        let add_meth = unsafe { counter_class.get_method(MethodIndex::new(0)) };
        let new_meth = unsafe { module_class.get_method(MethodIndex::new(0)) };

        assert_eq!(add_meth.hash, 123);
        assert_eq!(add_meth.registers, 2);
        assert_eq!(add_meth.instructions[0].opcode, Opcode::Return);
        assert_eq!(add_meth.instructions[0].arg(0), 2);

        assert_eq!(new_meth.hash, 456);
        assert_eq!(new_meth.registers, 0);
        assert_eq!(new_meth.instructions[0].opcode, Opcode::Return);
        assert_eq!(new_meth.instructions[0].arg(0), 2);
    }

    #[test]
    fn test_read_string() {
        let mut buffer = Vec::new();

        pack_string(&mut buffer, "inko");

        let output = read_string(reader!(buffer)).unwrap();

        assert_eq!(output, "inko".to_string());
    }

    #[test]
    fn test_read_string_too_large() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, u32::MAX);

        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_string_longer_than_size() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, 2);
        pack_signature(&mut buffer);

        let output = read_string(reader!(buffer)).unwrap();

        assert_eq!(output, "in".to_string());
    }

    #[test]
    fn test_read_string_invalid_utf8() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, 4);
        pack_u8(&mut buffer, 0);
        pack_u8(&mut buffer, 159);
        pack_u8(&mut buffer, 146);
        pack_u8(&mut buffer, 150);

        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_string_empty() {
        let buffer = Vec::new();
        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u8() {
        let mut buffer = Vec::new();

        pack_u8(&mut buffer, 2);

        let output = read_u8(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u8_empty() {
        let buffer = Vec::new();
        let output = read_u8(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u16() {
        let mut buffer = Vec::new();

        pack_u16(&mut buffer, 2);

        let output = read_u16(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u16_empty() {
        let buffer = Vec::new();
        let output = read_u16(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_i64() {
        let mut buffer = Vec::new();

        pack_i64(&mut buffer, 2);

        let output = read_i64(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_i64_empty() {
        let buffer = Vec::new();
        let output = read_i64(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_f64() {
        let mut buffer = Vec::new();

        pack_f64(&mut buffer, 2.123456);

        let output = read_f64(reader!(buffer)).unwrap();

        assert!((2.123456 - output).abs() < 0.00001);
    }

    #[test]
    fn test_read_f64_empty() {
        let buffer = Vec::new();
        let output = read_f64(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_instruction() {
        let mut buffer = Vec::new();
        let mut constants = Chunk::new(2);
        let const1 = Pointer::int(10);
        let const2 = Pointer::int(20);

        unsafe {
            constants.set(0, const1);
            constants.set(1, const2);
        }

        let u32_bytes = u32::to_le_bytes(1);
        let arg1 = u16::from_le_bytes([u32_bytes[0], u32_bytes[1]]);
        let arg2 = u16::from_le_bytes([u32_bytes[2], u32_bytes[3]]);

        pack_u8(&mut buffer, 49);
        pack_u16(&mut buffer, 14);
        pack_u16(&mut buffer, arg1);
        pack_u16(&mut buffer, arg2);

        let ins = read_instruction(reader!(buffer), &constants).unwrap();

        assert_eq!(ins.opcode, Opcode::GetConstant);
        assert_eq!(ins.arg(0), 14);
        assert_eq!(ins.u64_arg(1, 2, 3, 4), const2.as_ptr() as u64);
    }

    #[test]
    fn test_read_instructions() {
        let mut buffer = Vec::new();
        let constants = Chunk::new(0);

        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, 0);
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 4);

        let ins = read_instructions(reader!(buffer), &constants).unwrap();

        assert_eq!(ins.len(), 1);
        assert_eq!(ins[0].opcode, Opcode::Allocate);
        assert_eq!(ins[0].arg(0), 0);
        assert_eq!(ins[0].arg(1), 2);
        assert_eq!(ins[0].arg(2), 4);
    }

    #[test]
    fn test_read_class() {
        let class_index = 8;
        let mut buffer = Vec::new();
        let constants = Chunk::new(0);
        let perm = PermanentSpace::new(1, 1, MethodCounts::default());

        pack_u32(&mut buffer, class_index);
        pack_u8(&mut buffer, 0);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), &constants).is_ok());

        let class = unsafe { perm.get_class(ClassIndex::new(class_index)) };

        assert_eq!(&class.name, &"A");
    }

    #[test]
    fn test_read_class_with_process_class() {
        let class_index = 8;
        let mut buffer = Vec::new();
        let constants = Chunk::new(0);
        let perm = PermanentSpace::new(1, 1, MethodCounts::default());

        pack_u32(&mut buffer, class_index);
        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), &constants).is_ok());

        let class = unsafe { perm.get_class(ClassIndex::new(class_index)) };

        assert_eq!(&class.name, &"A");
    }

    #[test]
    fn test_read_class_with_builtin_class() {
        let mut buffer = Vec::new();
        let counts = MethodCounts { int_class: 1, ..Default::default() };
        let perm = PermanentSpace::new(0, 0, counts);
        let mut constants = Chunk::new(1);

        unsafe {
            constants.set(0, perm.allocate_string("add".to_string()));
        }

        pack_u32(&mut buffer, 0);
        pack_u8(&mut buffer, 2);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 0);
        pack_u16(&mut buffer, 1);

        pack_u16(&mut buffer, 0);
        pack_u32(&mut buffer, 123);
        pack_u16(&mut buffer, 0);

        // The instructions
        pack_u32(&mut buffer, 0);

        // The location table
        pack_u16(&mut buffer, 0);

        // The jump tables
        pack_u16(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), &constants).is_ok());
        assert_eq!(perm.int_class().method_slots, 1);
    }

    #[test]
    fn test_read_method() {
        let mut buffer = Vec::new();
        let mut constants = Chunk::new(2);
        let perm = PermanentSpace::new(0, 0, MethodCounts::default());
        let class = OwnedClass::new(Class::alloc("A".to_string(), 1, 0));

        unsafe { constants.set(0, perm.allocate_string("add".to_string())) };
        unsafe {
            constants.set(1, perm.allocate_string("test.inko".to_string()))
        };

        pack_u16(&mut buffer, 0);
        pack_u32(&mut buffer, 123);
        pack_u16(&mut buffer, 3);

        // The instructions
        pack_u32(&mut buffer, 0);

        // The location table
        pack_u16(&mut buffer, 1); // entries
        pack_u32(&mut buffer, 0);
        pack_u16(&mut buffer, 14);
        pack_u32(&mut buffer, 1);
        pack_u32(&mut buffer, 0);

        // The jump tables
        pack_u16(&mut buffer, 1);
        pack_u16(&mut buffer, 2);
        pack_u32(&mut buffer, 4);
        pack_u32(&mut buffer, 8);

        read_method(reader!(buffer), *class, &constants).unwrap();

        let method = unsafe { class.get_method(MethodIndex::new(0)) };

        assert_eq!(method.hash, 123);
        assert_eq!(method.registers, 3);

        let location = method.locations.get(0).unwrap();

        assert_eq!(unsafe { InkoString::read(&location.name) }, "add");
        assert_eq!(unsafe { InkoString::read(&location.file) }, "test.inko");
        assert_eq!(location.line, Pointer::int(14));
        assert_eq!(method.jump_tables, vec![vec![4, 8]]);
    }

    #[test]
    fn test_read_methods() {
        let mut buffer = Vec::new();
        let mut constants = Chunk::new(1);
        let perm = PermanentSpace::new(0, 0, MethodCounts::default());
        let class = OwnedClass::new(Class::alloc("A".to_string(), 2, 1));

        unsafe { constants.set(0, perm.allocate_string("add".to_string())) };

        pack_u16(&mut buffer, 1); // The number of methods to read

        pack_u16(&mut buffer, 1);
        pack_u32(&mut buffer, 123);
        pack_u16(&mut buffer, 1);

        // The instructions
        pack_u32(&mut buffer, 0);

        // The location table
        pack_u16(&mut buffer, 0);

        // The jump tables
        pack_u16(&mut buffer, 0);

        assert!(read_methods(reader!(buffer), *class, &constants).is_ok());
    }

    #[test]
    fn test_read_constants() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(0, 0, MethodCounts::default());

        pack_u32(&mut buffer, 3);

        pack_u32(&mut buffer, 0);
        pack_u8(&mut buffer, CONST_INTEGER);
        pack_i64(&mut buffer, -2);

        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, CONST_FLOAT);
        pack_f64(&mut buffer, 2.0);

        pack_u32(&mut buffer, 2);
        pack_u8(&mut buffer, CONST_STRING);
        pack_string(&mut buffer, "inko");

        let output = read_constants(&perm, reader!(buffer)).unwrap();

        assert_eq!(output.len(), 3);

        unsafe {
            assert_eq!(Int::read(*output.get(0)), -2);
            assert_eq!(Float::read(*output.get(1)), 2.0);
            assert_eq!(InkoString::read(output.get(2)), "inko");
        }
    }

    #[test]
    fn test_read_constant_invalid() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(0, 0, MethodCounts::default());

        pack_u8(&mut buffer, 255);

        assert!(read_constant(&perm, reader!(buffer)).is_err());
    }
}
