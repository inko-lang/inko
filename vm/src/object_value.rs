//! Values for Objects
//!
//! Objects need to be able to store values of different types such as floats or
//! strings. The ObjectValue enum can be used for storing such data and
//! operating on it.

use std::fs;
use std::mem;

use binding::RcBinding;
use block::Block;
use object_pointer::ObjectPointer;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Float(f64),
    String(Box<String>),
    InternedString(Box<String>),
    Array(Box<Vec<ObjectPointer>>),
    File(Box<fs::File>),
    Error(u16),
    Block(Box<Block>),
    Binding(RcBinding),
}

impl ObjectValue {
    pub fn is_none(&self) -> bool {
        match self {
            &ObjectValue::None => true,
            _ => false,
        }
    }

    pub fn is_float(&self) -> bool {
        match self {
            &ObjectValue::Float(_) => true,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        match self {
            &ObjectValue::Array(_) => true,
            _ => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match self {
            &ObjectValue::String(_) |
            &ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_interned_string(&self) -> bool {
        match self {
            &ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match self {
            &ObjectValue::File(_) => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match self {
            &ObjectValue::Error(_) => true,
            _ => false,
        }
    }

    pub fn is_block(&self) -> bool {
        match self {
            &ObjectValue::Block(_) => true,
            _ => false,
        }
    }

    pub fn is_binding(&self) -> bool {
        match self {
            &ObjectValue::Binding(_) => true,
            _ => false,
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match self {
            &ObjectValue::Float(val) => Ok(val),
            _ => Err("as_float called non a non float value".to_string()),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<ObjectPointer>, String> {
        match self {
            &ObjectValue::Array(ref val) => Ok(val),
            _ => Err("as_array called non a non array value".to_string()),
        }
    }

    pub fn as_array_mut(&mut self) -> Result<&mut Vec<ObjectPointer>, String> {
        match self {
            &mut ObjectValue::Array(ref mut val) => Ok(val),
            _ => Err("as_array_mut called on a non array".to_string()),
        }
    }

    pub fn as_string(&self) -> Result<&String, String> {
        match self {
            &ObjectValue::String(ref val) |
            &ObjectValue::InternedString(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_string() called on a non string".to_string())
            }
        }
    }

    pub fn as_file(&self) -> Result<&fs::File, String> {
        match self {
            &ObjectValue::File(ref val) => Ok(val),
            _ => Err("ObjectValue::as_file() called on a non file".to_string()),
        }
    }

    pub fn as_file_mut(&mut self) -> Result<&mut fs::File, String> {
        match self {
            &mut ObjectValue::File(ref mut val) => Ok(val),
            _ => {
                Err("ObjectValue::as_file_mut() called on a non file".to_string())
            }
        }
    }

    pub fn as_error(&self) -> Result<u16, String> {
        match self {
            &ObjectValue::Error(val) => Ok(val),
            _ => {
                Err("ObjectValue::as_error() called non a non error".to_string())
            }
        }
    }

    pub fn as_block(&self) -> Result<&Box<Block>, String> {
        match self {
            &ObjectValue::Block(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_block() called on a non block object"
                    .to_string())
            }
        }
    }

    pub fn as_binding(&self) -> Result<RcBinding, String> {
        match self {
            &ObjectValue::Binding(ref val) => Ok(val.clone()),
            _ => {
                Err("ObjectValue::as_binding() called non a non Binding"
                    .to_string())
            }
        }
    }

    pub fn take(&mut self) -> ObjectValue {
        mem::replace(self, ObjectValue::None)
    }

    /// Returns true if this value should be deallocated explicitly.
    pub fn should_deallocate_native(&self) -> bool {
        match self {
            &ObjectValue::None => false,
            _ => true,
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
    ObjectValue::String(Box::new(value))
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

pub fn error(value: u16) -> ObjectValue {
    ObjectValue::Error(value)
}

pub fn block(value: Block) -> ObjectValue {
    ObjectValue::Block(Box::new(value))
}

pub fn binding(value: RcBinding) -> ObjectValue {
    ObjectValue::Binding(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use deref_pointer::DerefPointer;
    use block::Block;
    use binding::Binding;
    use compiled_code::CompiledCode;
    use global_scope::{GlobalScope, GlobalScopeReference};
    use object_pointer::ObjectPointer;

    #[test]
    fn test_is_none() {
        assert!(ObjectValue::None.is_none());
        assert_eq!(ObjectValue::Float(10.0).is_none(), false);
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
        assert!(ObjectValue::String(Box::new(String::new())).is_string());
        assert_eq!(ObjectValue::None.is_string(), false);
    }

    #[test]
    fn test_is_string_with_interned_string() {
        assert!(ObjectValue::InternedString(Box::new(String::new())).is_string());
    }

    #[test]
    fn test_is_interned_string() {
        assert!(ObjectValue::InternedString(Box::new(String::new()))
            .is_interned_string());
    }

    #[test]
    fn test_is_interned_string_with_regular_string() {
        assert_eq!(ObjectValue::String(Box::new(String::new()))
                       .is_interned_string(),
                   false);
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_is_file() {
        let file = Box::new(File::open("/dev/null").unwrap());

        assert!(ObjectValue::File(file).is_file());
        assert_eq!(ObjectValue::None.is_file(), false);
    }

    #[test]
    fn test_is_error() {
        assert!(ObjectValue::Error(2).is_error());
        assert_eq!(ObjectValue::None.is_error(), false);
    }

    #[test]
    fn test_is_block() {
        let code = CompiledCode::new("a".to_string(),
                                     "a.inko".to_string(),
                                     1,
                                     Vec::new());

        let binding = Binding::new();
        let scope = GlobalScope::new();
        let block = Block::new(DerefPointer::new(&code),
                               binding,
                               GlobalScopeReference::new(&scope));

        assert!(ObjectValue::Block(Box::new(block)).is_block());
        assert_eq!(ObjectValue::None.is_block(), false);
    }

    #[test]
    fn test_is_binding() {
        let binding = Binding::new();

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
        let string = Box::new("test".to_string());
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
    fn test_as_error_without_error() {
        assert!(ObjectValue::None.as_error().is_err());
    }

    #[test]
    fn test_as_error_with_error() {
        let result = ObjectValue::Error(2).as_error();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn test_as_block_without_block() {
        assert!(ObjectValue::None.as_block().is_err());
    }

    #[test]
    fn test_as_block_with_block() {
        let code = CompiledCode::new("a".to_string(),
                                     "a.inko".to_string(),
                                     1,
                                     Vec::new());
        let binding = Binding::new();
        let scope = GlobalScope::new();
        let block = Block::new(DerefPointer::new(&code),
                               binding,
                               GlobalScopeReference::new(&scope));

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
        let pointer = ObjectPointer::null();
        let binding = Binding::new();

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
    fn test_error() {
        assert!(error(2).is_error());
    }

    #[test]
    fn test_block() {
        let code = CompiledCode::new("a".to_string(),
                                     "a.inko".to_string(),
                                     1,
                                     Vec::new());

        let binding = Binding::new();
        let scope = GlobalScope::new();

        let blk = Block::new(DerefPointer::new(&code),
                             binding,
                             GlobalScopeReference::new(&scope));

        assert!(block(blk).is_block());
    }

    #[test]
    fn test_binding() {
        let b = Binding::new();

        assert!(binding(b).is_binding());
    }
}
