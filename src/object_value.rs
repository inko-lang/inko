use object::RcObject;
use thread::RcThread;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    String(Box<String>),
    Array(Box<Vec<RcObject>>),
    Thread(RcThread)
}

impl ObjectValue {
    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _                       => false
        }
    }

    pub fn is_float(&self) -> bool {
        match *self {
            ObjectValue::Float(_) => true,
            _                     => false
        }
    }

    pub fn is_array(&self) -> bool {
        match *self {
            ObjectValue::Array(_) => true,
            _                     => false
        }
    }

    pub fn as_integer(&self) -> isize {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => {
                panic!("ObjectValue::as_integer() called on a non integer");
            }
        }
    }

    pub fn as_float(&self) -> f64 {
        match *self {
            ObjectValue::Float(val) => val,
            _ => {
                panic!("ObjectValue::as_float() called on a non float");
            }
        }
    }

    pub fn as_array(&self) -> &Vec<RcObject> {
        match *self {
            ObjectValue::Array(ref val) => val,
            _ => {
                panic!("ObjectValue::as_Array() called on a non array");
            }
        }
    }

    pub fn as_array_mut(&mut self) -> &mut Vec<RcObject> {
        match *self {
            ObjectValue::Array(ref mut val) => val,
            _ => {
                panic!("ObjectValue::as_array_mut() called on a non array");
            }
        }
    }

    pub fn as_thread(&self) -> RcThread {
        match *self {
            ObjectValue::Thread(ref val) => {
                val.clone()
            },
            _ => {
                panic!("ObjectValue::as_thread() called on a non thread");
            }
        }
    }
}

pub fn none() -> ObjectValue {
    ObjectValue::None
}

pub fn thread(value: RcThread) -> ObjectValue {
    ObjectValue::Thread(value)
}

pub fn integer(value: isize) -> ObjectValue {
    ObjectValue::Integer(value)
}

pub fn float(value: f64) -> ObjectValue {
    ObjectValue::Float(value)
}

pub fn string(value: String) -> ObjectValue {
    ObjectValue::String(Box::new(value))
}

pub fn array(value: Vec<RcObject>) -> ObjectValue {
    ObjectValue::Array(Box::new(value))
}
