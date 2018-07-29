//! Values for Objects
//!
//! Objects need to be able to store values of different types such as floats or
//! strings. The ObjectValue enum can be used for storing such data and
//! operating on it.

use num_bigint::BigInt;
use std::fs;
use std::mem;

use arc_without_weak::ArcWithoutWeak;
use binding::RcBinding;
use block::Block;
use hasher::Hasher;
use object_pointer::ObjectPointer;

/// Enum for storing different values in an Object.
#[cfg_attr(feature = "cargo-clippy", allow(box_vec))]
pub enum ObjectValue {
    None,
    Float(f64),

    /// Strings use an Arc so they can be sent to other processes without
    /// requiring a full copy of the data.
    String(ArcWithoutWeak<String>),

    /// An interned string is a string allocated on the permanent space. For
    /// every unique interned string there is only one object allocated.
    InternedString(Box<String>),
    Array(Box<Vec<ObjectPointer>>),
    File(Box<fs::File>),
    Block(Box<Block>),
    Binding(RcBinding),

    /// An arbitrary precision integer stored on the heap.
    BigInt(Box<BigInt>),

    /// A heap allocated integer that doesn't fit in a tagged pointer, but is
    /// too small for a BigInt.
    Integer(i64),

    /// A heap allocator hasher used for hashing objects.
    Hasher(Box<Hasher>),

    /// An Array of bytes, typically produced by reading from a stream of sorts.
    ByteArray(Box<Vec<u8>>),
}

impl ObjectValue {
    pub fn is_none(&self) -> bool {
        match *self {
            ObjectValue::None => true,
            _ => false,
        }
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn is_float(&self) -> bool {
        match *self {
            ObjectValue::Float(_) => true,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        match *self {
            ObjectValue::Array(_) => true,
            _ => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match *self {
            ObjectValue::String(_) | ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_interned_string(&self) -> bool {
        match *self {
            ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match *self {
            ObjectValue::File(_) => true,
            _ => false,
        }
    }

    pub fn is_block(&self) -> bool {
        match *self {
            ObjectValue::Block(_) => true,
            _ => false,
        }
    }

    pub fn is_binding(&self) -> bool {
        match *self {
            ObjectValue::Binding(_) => true,
            _ => false,
        }
    }

    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _ => false,
        }
    }

    pub fn is_bigint(&self) -> bool {
        match *self {
            ObjectValue::BigInt(_) => true,
            _ => false,
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match *self {
            ObjectValue::Float(val) => Ok(val),
            _ => Err("as_float called non a non float value".to_string()),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref val) => Ok(val),
            _ => Err("as_array called non a non array value".to_string()),
        }
    }

    pub fn as_array_mut(&mut self) -> Result<&mut Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref mut val) => Ok(val),
            _ => Err("as_array_mut called on a non array".to_string()),
        }
    }

    pub fn as_byte_array(&self) -> Result<&Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref val) => Ok(val),
            _ => Err(
                "as_byte_array called non a non byte array value".to_string()
            ),
        }
    }

    pub fn as_byte_array_mut(&mut self) -> Result<&mut Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref mut val) => Ok(val),
            _ => {
                Err("as_byte_array_mut called on a non byte array".to_string())
            }
        }
    }

    pub fn as_string(&self) -> Result<&String, String> {
        match *self {
            ObjectValue::String(ref val) => Ok(val),
            ObjectValue::InternedString(ref val) => Ok(val),
            _ => Err(
                "ObjectValue::as_string() called on a non string".to_string()
            ),
        }
    }

    pub fn as_file(&self) -> Result<&fs::File, String> {
        match *self {
            ObjectValue::File(ref val) => Ok(val),
            _ => Err("ObjectValue::as_file() called on a non file".to_string()),
        }
    }

    pub fn as_file_mut(&mut self) -> Result<&mut fs::File, String> {
        match *self {
            ObjectValue::File(ref mut val) => Ok(val),
            _ => Err(
                "ObjectValue::as_file_mut() called on a non file".to_string()
            ),
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(borrowed_box))]
    pub fn as_block(&self) -> Result<&Box<Block>, String> {
        match *self {
            ObjectValue::Block(ref val) => Ok(val),
            _ => Err("ObjectValue::as_block() called on a non block object"
                .to_string()),
        }
    }

    pub fn as_binding(&self) -> Result<RcBinding, String> {
        match *self {
            ObjectValue::Binding(ref val) => Ok(val.clone()),
            _ => Err("ObjectValue::as_binding() called non a non Binding"
                .to_string()),
        }
    }

    pub fn as_bigint(&self) -> Result<&BigInt, String> {
        match *self {
            ObjectValue::BigInt(ref val) => Ok(val),
            _ => Err(
                "ObjectValue::as_bigint() called on a non BigInt".to_string()
            ),
        }
    }

    pub fn as_integer(&self) -> Result<i64, String> {
        match *self {
            ObjectValue::Integer(val) => Ok(val),
            _ => Err(
                "ObjectValue::as_integer() called on a non integer".to_string()
            ),
        }
    }

    pub fn as_hasher_mut(&mut self) -> Result<&mut Hasher, String> {
        match *self {
            ObjectValue::Hasher(ref mut val) => Ok(val),
            _ => Err("ObjectValue::as_hasher_mut() called on a non hasher"
                .to_string()),
        }
    }

    pub fn take(&mut self) -> ObjectValue {
        mem::replace(self, ObjectValue::None)
    }

    /// Returns true if this value should be deallocated explicitly.
    pub fn should_deallocate_native(&self) -> bool {
        match *self {
            ObjectValue::None => false,
            _ => true,
        }
    }

    pub fn is_immutable(&self) -> bool {
        match *self {
            ObjectValue::Float(_)
            | ObjectValue::Integer(_)
            | ObjectValue::String(_)
            | ObjectValue::BigInt(_)
            | ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }
}

pub fn none() -> ObjectValue {
    ObjectValue::None
}

pub fn float(value: f64) -> ObjectValue {
    ObjectValue::Float(value)
}

pub fn string(value: String) -> ObjectValue {
    ObjectValue::String(ArcWithoutWeak::new(value))
}

pub fn interned_string(value: String) -> ObjectValue {
    ObjectValue::InternedString(Box::new(value))
}

pub fn array(value: Vec<ObjectPointer>) -> ObjectValue {
    ObjectValue::Array(Box::new(value))
}

pub fn file(value: fs::File) -> ObjectValue {
    ObjectValue::File(Box::new(value))
}

pub fn block(value: Block) -> ObjectValue {
    ObjectValue::Block(Box::new(value))
}

pub fn binding(value: RcBinding) -> ObjectValue {
    ObjectValue::Binding(value)
}

pub fn bigint(value: BigInt) -> ObjectValue {
    ObjectValue::BigInt(Box::new(value))
}

pub fn integer(value: i64) -> ObjectValue {
    ObjectValue::Integer(value)
}

pub fn hasher(value: Hasher) -> ObjectValue {
    ObjectValue::Hasher(Box::new(value))
}

pub fn byte_array(value: Vec<u8>) -> ObjectValue {
    ObjectValue::ByteArray(Box::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use binding::Binding;
    use block::Block;
    use compiled_code::CompiledCode;
    use config::Config;
    use deref_pointer::DerefPointer;
    use global_scope::{GlobalScope, GlobalScopePointer};
    use object_pointer::ObjectPointer;
    use std::fs::File;
    use vm::state::{RcState, State};

    fn state() -> RcState {
        State::new(Config::new())
    }

    #[test]
    fn test_is_none() {
        assert!(ObjectValue::None.is_none());
        assert_eq!(ObjectValue::Float(10.0).is_none(), false);
    }

    #[test]
    fn test_is_some() {
        assert_eq!(ObjectValue::None.is_some(), false);
        assert!(ObjectValue::Float(1.0).is_some());
    }

    #[test]
    fn test_is_float() {
        assert!(ObjectValue::Float(10.5).is_float());
        assert_eq!(ObjectValue::None.is_float(), false);
    }

    #[test]
    fn test_is_array() {
        assert!(ObjectValue::Array(Box::new(Vec::new())).is_array());
        assert_eq!(ObjectValue::None.is_array(), false);
    }

    #[test]
    fn test_is_string() {
        assert!(
            ObjectValue::String(ArcWithoutWeak::new(String::new())).is_string()
        );
        assert_eq!(ObjectValue::None.is_string(), false);
    }

    #[test]
    fn test_is_string_with_interned_string() {
        assert!(
            ObjectValue::InternedString(Box::new(String::new())).is_string()
        );
    }

    #[test]
    fn test_is_interned_string() {
        assert!(
            ObjectValue::InternedString(Box::new(String::new()))
                .is_interned_string()
        );
    }

    #[test]
    fn test_is_interned_string_with_regular_string() {
        assert_eq!(
            ObjectValue::String(ArcWithoutWeak::new(String::new()))
                .is_interned_string(),
            false
        );
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_is_file() {
        let file = Box::new(File::open("/dev/null").unwrap());

        assert!(ObjectValue::File(file).is_file());
        assert_eq!(ObjectValue::None.is_file(), false);
    }

    #[test]
    fn test_is_block() {
        let state = state();
        let code = CompiledCode::new(
            state.intern_owned("a".to_string()),
            state.intern_owned("a.inko".to_string()),
            1,
            Vec::new(),
        );

        let binding = Binding::new(0);
        let scope = GlobalScope::new();
        let block = Block::new(
            DerefPointer::new(&code),
            binding,
            GlobalScopePointer::new(&scope),
        );

        assert!(ObjectValue::Block(Box::new(block)).is_block());
        assert_eq!(ObjectValue::None.is_block(), false);
    }

    #[test]
    fn test_is_binding() {
        let binding = Binding::new(0);

        assert!(ObjectValue::Binding(binding).is_binding());
        assert_eq!(ObjectValue::None.is_binding(), false);
    }

    #[test]
    fn test_as_float_without_float() {
        assert!(ObjectValue::None.as_float().is_err());
    }

    #[test]
    fn test_as_float_with_float() {
        let result = ObjectValue::Float(10.5).as_float();

        assert!(result.is_ok());
        assert!(result.unwrap() == 10.5);
    }

    #[test]
    fn test_as_array_without_array() {
        assert!(ObjectValue::None.as_array().is_err());
    }

    #[test]
    fn test_as_array_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);
        let value = ObjectValue::Array(array);
        let result = value.as_array();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_array_mut_without_array() {
        assert!(ObjectValue::None.as_array_mut().is_err());
    }

    #[test]
    fn test_as_array_mut_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);
        let mut value = ObjectValue::Array(array);
        let result = value.as_array_mut();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_string_without_string() {
        assert!(ObjectValue::None.as_string().is_err());
    }

    #[test]
    fn test_as_string_with_string() {
        let string = ArcWithoutWeak::new("test".to_string());
        let value = ObjectValue::String(string);
        let result = value.as_string();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), &"test".to_string());
    }

    #[test]
    fn test_as_file_without_file() {
        assert!(ObjectValue::None.as_file().is_err());
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_as_file_with_file() {
        let file = Box::new(File::open("/dev/null").unwrap());
        let value = ObjectValue::File(file);
        let result = value.as_file();

        assert!(result.is_ok());
    }

    #[test]
    fn test_as_file_mut_without_file() {
        assert!(ObjectValue::None.as_file_mut().is_err());
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_as_file_mut_with_file() {
        let file = Box::new(File::open("/dev/null").unwrap());
        let mut value = ObjectValue::File(file);
        let result = value.as_file_mut();

        assert!(result.is_ok());
    }

    #[test]
    fn test_as_block_without_block() {
        assert!(ObjectValue::None.as_block().is_err());
    }

    #[test]
    fn test_as_block_with_block() {
        let state = state();
        let code = CompiledCode::new(
            state.intern_owned("a".to_string()),
            state.intern_owned("a.inko".to_string()),
            1,
            Vec::new(),
        );
        let binding = Binding::new(0);
        let scope = GlobalScope::new();
        let block = Block::new(
            DerefPointer::new(&code),
            binding,
            GlobalScopePointer::new(&scope),
        );

        let value = ObjectValue::Block(Box::new(block));
        let result = value.as_block();

        assert!(result.is_ok());
    }

    #[test]
    fn test_as_binding_without_binding() {
        assert!(ObjectValue::None.as_binding().is_err());
    }

    #[test]
    fn test_as_binding_with_binding() {
        let pointer = ObjectPointer::integer(5);
        let binding = Binding::new(1);

        binding.set_local(0, pointer);

        let result = ObjectValue::Binding(binding).as_binding();

        assert!(result.is_ok());
        assert!(result.unwrap().get_local(0) == pointer);
    }

    #[test]
    fn test_take() {
        let mut val1 = ObjectValue::Float(5.0);
        let val2 = val1.take();

        assert!(val1.is_none());
        assert!(val2.is_float());
        assert_eq!(val2.as_float().unwrap(), 5.0);
    }

    #[test]
    fn test_none() {
        assert!(none().is_none());
    }

    #[test]
    fn test_float() {
        assert!(float(10.5).is_float());
    }

    #[test]
    fn test_string() {
        assert!(string("a".to_string()).is_string());
    }

    #[test]
    fn test_array() {
        assert!(array(Vec::new()).is_array());
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_file() {
        let f = File::open("/dev/null").unwrap();

        assert!(file(f).is_file());
    }

    #[test]
    fn test_block() {
        let state = state();
        let code = CompiledCode::new(
            state.intern_owned("a".to_string()),
            state.intern_owned("a.inko".to_string()),
            1,
            Vec::new(),
        );

        let binding = Binding::new(0);
        let scope = GlobalScope::new();

        let blk = Block::new(
            DerefPointer::new(&code),
            binding,
            GlobalScopePointer::new(&scope),
        );

        assert!(block(blk).is_block());
    }

    #[test]
    fn test_binding() {
        let b = Binding::new(0);

        assert!(binding(b).is_binding());
    }

    #[test]
    fn test_is_immutable() {
        assert!(string("a".to_string()).is_immutable());
        assert!(float(10.5).is_immutable());
        assert!(interned_string("a".to_string()).is_immutable());
    }

    #[test]
    fn test_as_byte_array_without_byte_array() {
        assert!(ObjectValue::None.as_byte_array().is_err());
    }

    #[test]
    fn test_as_byte_array_with_byte_array() {
        let byte_array = Box::new(vec![1]);
        let value = ObjectValue::ByteArray(byte_array);
        let result = value.as_byte_array();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_byte_array_mut_without_byte_array() {
        assert!(ObjectValue::None.as_byte_array_mut().is_err());
    }

    #[test]
    fn test_as_byte_array_mut_with_byte_array() {
        let byte_array = Box::new(vec![1]);
        let mut value = ObjectValue::ByteArray(byte_array);
        let result = value.as_byte_array_mut();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
