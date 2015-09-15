use object::RcObject;
use thread::RcThread;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    ByteArray(Box<Vec<u8>>),
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

    pub fn as_integer(&self) -> isize {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => {
                panic!("ObjectValue::as_integer() called on a non integer");
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

// TODO: remove me once strings are just regular arrays of integers.
pub fn byte_array(value: Vec<u8>) -> ObjectValue {
    ObjectValue::ByteArray(Box::new(value))
}

pub fn array(value: Vec<RcObject>) -> ObjectValue {
    ObjectValue::Array(Box::new(value))
}
