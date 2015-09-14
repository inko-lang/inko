use std::mem;

use object::RcObject;
use thread::RcThread;

pub type ValuePointer = *const u8;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    ByteArray(ValuePointer),
    Array(ValuePointer),
    Thread(ValuePointer)
}

unsafe impl Send for ObjectValue {}
unsafe impl Sync for ObjectValue {}

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
            ObjectValue::Thread(val) => {
                unsafe { mem::transmute::<ValuePointer, RcThread>(val).clone() }
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
    unsafe {
        let pointer = mem::transmute::<RcThread, ValuePointer>(value);

        ObjectValue::Thread(pointer)
    }
}

pub fn integer(value: isize) -> ObjectValue {
    ObjectValue::Integer(value)
}

pub fn float(value: f64) -> ObjectValue {
    ObjectValue::Float(value)
}

// TODO: remove me once strings are just regular arrays of integers.
pub fn byte_array(value: Vec<u8>) -> ObjectValue {
    let boxed = Box::new(value);

    unsafe {
        let pointer = mem::transmute::<Box<Vec<u8>>, ValuePointer>(boxed);

        ObjectValue::ByteArray(pointer)
    }
}

pub fn array(value: Vec<RcObject>) -> ObjectValue {
    let boxed = Box::new(value);

    unsafe {
        let pointer = mem::transmute::<Box<Vec<RcObject>>, ValuePointer>(boxed);

        ObjectValue::Array(pointer)
    }
}
