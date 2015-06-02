use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use class::RcClass;
use compiled_code::RcCompiledCode;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    String(String),
    Array(Vec<RcObject>)
}

/// A mutable, reference counted Object.
pub type RcObject = Rc<RefCell<Object>>;

/// An Object/instance structure, optionally with an associated value.
///
/// An Object in the VM is used to represent an instance of some Class in the
/// Aeon language. For example, the string "foo" is stored as an Object in the
/// VM mapped to the corresponding Class (using the Class struct in the VM).
///
pub struct Object {
    /// The class of the object.
    pub class: RcClass,

    /// The instance variables of the object. These don't use a lock as objects
    /// can't be modified from multiple threads in parallel.
    pub instance_variables: HashMap<String, RcObject>,

    /// A value associated with the object, if any.
    pub value: ObjectValue,

    /// When set to "true" this object won't be GC'd.
    pub pinned: bool
}

impl Object {
    /// Creates a regular Object without using an Rc.
    pub fn new(class: RcClass, value: ObjectValue) -> Object {
        Object {
            class: class,
            instance_variables: HashMap::new(),
            value: value,
            pinned: false
        }
    }

    /// Creates a mutable, reference counted Object.
    pub fn with_rc(class: RcClass, value: ObjectValue) -> RcObject {
        Rc::new(RefCell::new(Object::new(class, value)))
    }

    /// Looks up and returns a method for the given name.
    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let class_ref = self.class.borrow();

        class_ref.lookup_method(name)
    }

    /// Returns an error message for undefined method calls.
    pub fn undefined_method_error(&self, name: &String) -> String {
        let class_ref = self.class.borrow();

        match class_ref.name() {
            Some(class_name) => {
                format!("Undefined method {} called on a {}", name, class_name)
            },
            None => {
                format!("Undefined method {} called", name)
            }
        }
    }

    /// Returns an error message for private method calls.
    pub fn private_method_error(&self, name: &String) -> String {
        let class_ref = self.class.borrow();

        match class_ref.name() {
            Some(class_name) => {
                format!("Private method {} called on a {}", name, class_name)
            },
            None => {
                format!("Private method {} called", name)
            }
        }
    }
}
