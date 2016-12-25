//! Values for Objects
//!
//! Objects need to be able to store values of different types such as integers
//! or strings. The ObjectValue enum can be used for storing such data and
//! operating on it.

use std::fs;
use std::mem;

use binding::RcBinding;
use object_pointer::ObjectPointer;
use compiled_code::RcCompiledCode;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(i64),
    Float(f64),
    String(Box<String>),
    Array(Box<Vec<ObjectPointer>>),
    File(Box<fs::File>),
    Error(u16),
    CompiledCode(RcCompiledCode),
    Binding(RcBinding),
}

impl ObjectValue {
    pub fn is_none(&self) -> bool {
        match *self {
            ObjectValue::None => true,
            _ => false,
        }
    }

    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _ => false,
        }
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
            ObjectValue::String(_) => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match *self {
            ObjectValue::File(_) => true,
            _ => false,
        }
    }

    pub fn is_error(&self) -> bool {
        match *self {
            ObjectValue::Error(_) => true,
            _ => false,
        }
    }

    pub fn is_compiled_code(&self) -> bool {
        match *self {
            ObjectValue::CompiledCode(_) => true,
            _ => false,
        }
    }

    pub fn is_binding(&self) -> bool {
        match *self {
            ObjectValue::Binding(_) => true,
            _ => false,
        }
    }

    pub fn as_integer(&self) -> Result<i64, String> {
        match *self {
            ObjectValue::Integer(val) => Ok(val),
            _ => Err("as_integer called on a non integer value".to_string()),
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

    pub fn as_string(&self) -> Result<&String, String> {
        match *self {
            ObjectValue::String(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_string() called on a non string".to_string())
            }
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
            _ => {
                Err("ObjectValue::as_file_mut() called on a non file".to_string())
            }
        }
    }

    pub fn as_error(&self) -> Result<u16, String> {
        match *self {
            ObjectValue::Error(val) => Ok(val),
            _ => {
                Err("ObjectValue::as_error() called non a non error".to_string())
            }
        }
    }

    pub fn as_compiled_code(&self) -> Result<RcCompiledCode, String> {
        match *self {
            ObjectValue::CompiledCode(ref val) => Ok(val.clone()),
            _ => {
                Err("ObjectValue::as_compiled_code() called on a non compiled \
                     code object"
                    .to_string())
            }
        }
    }

    pub fn as_binding(&self) -> Result<RcBinding, String> {
        match *self {
            ObjectValue::Binding(ref val) => Ok(val.clone()),
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

pub fn integer(value: i64) -> ObjectValue {
    ObjectValue::Integer(value)
}

pub fn float(value: f64) -> ObjectValue {
    ObjectValue::Float(value)
}

pub fn string(value: String) -> ObjectValue {
    ObjectValue::String(Box::new(value))
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

pub fn compiled_code(value: RcCompiledCode) -> ObjectValue {
    ObjectValue::CompiledCode(value)
}

pub fn binding(value: RcBinding) -> ObjectValue {
    ObjectValue::Binding(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use binding::Binding;
    use compiled_code::CompiledCode;
    use object_pointer::ObjectPointer;

    #[test]
    fn test_is_none() {
        assert!(ObjectValue::None.is_none());
        assert_eq!(ObjectValue::Integer(10).is_none(), false);
    }

    #[test]
    fn test_is_integer() {
        assert!(ObjectValue::Integer(10).is_integer());
        assert_eq!(ObjectValue::None.is_integer(), false);
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
    fn test_is_compiled_code() {
        let code = CompiledCode::with_rc("a".to_string(),
                                         "a.aeon".to_string(),
                                         1,
                                         Vec::new());

        assert!(ObjectValue::CompiledCode(code).is_compiled_code());
        assert_eq!(ObjectValue::None.is_compiled_code(), false);
    }

    #[test]
    fn test_is_binding() {
        let binding = Binding::new(ObjectPointer::null());

        assert!(ObjectValue::Binding(binding).is_binding());
        assert_eq!(ObjectValue::None.is_binding(), false);
    }

    #[test]
    fn test_as_integer_without_integer() {
        assert!(ObjectValue::None.as_integer().is_err());
    }

    #[test]
    fn test_as_integer_with_integer() {
        let result = ObjectValue::Integer(10).as_integer();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10);
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
    fn test_as_compiled_code_without_code() {
        assert!(ObjectValue::None.as_compiled_code().is_err());
    }

    #[test]
    fn test_as_compiled_code_with_compiled_code() {
        let code = CompiledCode::with_rc("a".to_string(),
                                         "a.aeon".to_string(),
                                         1,
                                         Vec::new());
        let result = ObjectValue::CompiledCode(code).as_compiled_code();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "a".to_string());
    }

    #[test]
    fn test_as_binding_without_binding() {
        assert!(ObjectValue::None.as_binding().is_err());
    }

    #[test]
    fn test_as_binding_with_binding() {
        let pointer = ObjectPointer::null();
        let binding = Binding::new(pointer);
        let result = ObjectValue::Binding(binding).as_binding();

        assert!(result.is_ok());
        assert!(result.unwrap().self_object == pointer);
    }

    #[test]
    fn test_take() {
        let mut val1 = ObjectValue::Integer(5);
        let val2 = val1.take();

        assert!(val1.is_none());
        assert!(val2.is_integer());
        assert_eq!(val2.as_integer().unwrap(), 5);
    }

    #[test]
    fn test_none() {
        assert!(none().is_none());
    }

    #[test]
    fn test_integer() {
        assert!(integer(10).is_integer());
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
    fn test_compiled_code() {
        let code = CompiledCode::with_rc("a".to_string(),
                                         "a.aeon".to_string(),
                                         1,
                                         Vec::new());

        assert!(compiled_code(code).is_compiled_code());
    }

    #[test]
    fn test_binding() {
        let b = Binding::new(ObjectPointer::null());

        assert!(binding(b).is_binding());
    }
}
