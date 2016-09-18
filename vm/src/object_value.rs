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

    pub fn type_name(&self) -> &str {
        match *self {
            ObjectValue::None => "Object",
            ObjectValue::Integer(_) => "Integer",
            ObjectValue::Float(_) => "Float",
            ObjectValue::String(_) => "String",
            ObjectValue::Array(_) => "Array",
            ObjectValue::File(_) => "File",
            ObjectValue::Error(_) => "Error",
            ObjectValue::CompiledCode(_) => "CompiledCode",
            ObjectValue::Binding(_) => "Binding",
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
