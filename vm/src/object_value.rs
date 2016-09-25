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

    pub fn as_integer(&self) -> i64 {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => panic!("ObjectValue::as_integer() called on a non integer"),
        }
    }

    pub fn as_float(&self) -> f64 {
        match *self {
            ObjectValue::Float(val) => val,
            _ => panic!("ObjectValue::as_float() called on a non float"),
        }
    }

    pub fn as_array(&self) -> &Vec<ObjectPointer> {
        match *self {
            ObjectValue::Array(ref val) => val,
            _ => panic!("ObjectValue::as_Array() called on a non array"),
        }
    }

    pub fn as_array_mut(&mut self) -> &mut Vec<ObjectPointer> {
        match *self {
            ObjectValue::Array(ref mut val) => val,
            _ => panic!("ObjectValue::as_array_mut() called on a non array"),
        }
    }

    pub fn as_string(&self) -> &String {
        match *self {
            ObjectValue::String(ref val) => val,
            _ => panic!("ObjectValue::as_string() called on a non string"),
        }
    }

    pub fn as_file(&self) -> &fs::File {
        match *self {
            ObjectValue::File(ref val) => val,
            _ => panic!("ObjectValue::as_file() called on a non file"),
        }
    }

    pub fn as_file_mut(&mut self) -> &mut fs::File {
        match *self {
            ObjectValue::File(ref mut val) => val,
            _ => panic!("ObjectValue::as_file_mut() called on a non file"),
        }
    }

    pub fn as_error(&self) -> u16 {
        match *self {
            ObjectValue::Error(val) => val,
            _ => panic!("ObjectValue::as_error() called non a non error"),
        }
    }

    pub fn as_compiled_code(&self) -> RcCompiledCode {
        match *self {
            ObjectValue::CompiledCode(ref val) => val.clone(),
            _ => {
                panic!("ObjectValue::as_compiled_code() called on a non \
                        compiled code object")
            }
        }
    }

    pub fn as_binding(&self) -> RcBinding {
        match *self {
            ObjectValue::Binding(ref val) => val.clone(),
            _ => panic!("ObjectValue::as_binding() called non a non Binding"),
        }
    }

    pub fn take(&mut self) -> ObjectValue {
        mem::replace(self, ObjectValue::None)
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
    #[should_panic]
    fn test_as_integer_without_integer() {
        ObjectValue::None.as_integer();
    }

    #[test]
    fn test_as_integer_with_integer() {
        assert_eq!(ObjectValue::Integer(10).as_integer(), 10);
    }

    #[test]
    #[should_panic]
    fn test_as_float_without_float() {
        ObjectValue::None.as_float();
    }

    #[test]
    fn test_as_float_with_float() {
        assert!(ObjectValue::Float(10.5).as_float() == 10.5)
    }

    #[test]
    #[should_panic]
    fn test_as_array_without_array() {
        ObjectValue::None.as_array();
    }

    #[test]
    fn test_as_array_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);

        assert_eq!(ObjectValue::Array(array).as_array().len(), 1);
    }

    #[test]
    #[should_panic]
    fn test_as_array_mut_without_array() {
        ObjectValue::None.as_array_mut();
    }

    #[test]
    fn test_as_array_mut_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);

        assert_eq!(ObjectValue::Array(array).as_array_mut().len(), 1);
    }

    #[test]
    #[should_panic]
    fn test_as_string_without_string() {
        ObjectValue::None.as_string();
    }

    #[test]
    fn test_as_string_with_string() {
        let string = Box::new("test".to_string());

        assert_eq!(ObjectValue::String(string).as_string(), &"test".to_string());
    }

    #[test]
    #[should_panic]
    fn test_as_file_without_file() {
        ObjectValue::None.as_file();
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_as_file_with_file() {
        let file = Box::new(File::open("/dev/null").unwrap());

        ObjectValue::File(file).as_file();
    }

    #[test]
    #[should_panic]
    fn test_as_file_mut_without_file() {
        ObjectValue::None.as_file_mut();
    }

    #[test]
    #[cfg(not(platform = "windows"))]
    fn test_as_file_mut_with_file() {
        let file = Box::new(File::open("/dev/null").unwrap());

        ObjectValue::File(file).as_file_mut();
    }

    #[test]
    #[should_panic]
    fn test_as_error_without_error() {
        ObjectValue::None.as_error();
    }

    #[test]
    fn test_as_error_with_error() {
        assert_eq!(ObjectValue::Error(2).as_error(), 2);
    }

    #[test]
    #[should_panic]
    fn test_as_compiled_code_without_code() {
        ObjectValue::None.as_compiled_code();
    }

    #[test]
    fn test_as_compiled_code_with_compiled_code() {
        let code = CompiledCode::with_rc("a".to_string(),
                                         "a.aeon".to_string(),
                                         1,
                                         Vec::new());

        assert_eq!(ObjectValue::CompiledCode(code).as_compiled_code().name,
                   "a".to_string());
    }

    #[test]
    #[should_panic]
    fn test_as_binding_without_binding() {
        ObjectValue::None.as_binding();
    }

    #[test]
    fn test_as_binding_with_binding() {
        let pointer = ObjectPointer::null();
        let binding = Binding::new(pointer);

        assert!(ObjectValue::Binding(binding).as_binding().self_object ==
                pointer);
    }

    #[test]
    fn test_take() {
        let mut val1 = ObjectValue::Integer(5);
        let val2 = val1.take();

        assert!(val1.is_none());
        assert!(val2.is_integer());
        assert_eq!(val2.as_integer(), 5);
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
