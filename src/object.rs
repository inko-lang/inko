//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use compiled_code::RcCompiledCode;
use thread::RcThread;

/// Enum for storing different values in an Object.
pub enum ObjectValue {
    None,
    Integer(isize),
    Float(f64),
    ByteArray(Vec<u8>),
    Array(Vec<RcObject>),
    Thread(RcThread)
}

impl ObjectValue {
    /// Returns true if the current value is an ObjectValue::Integer.
    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _                       => false
        }
    }

    /// Returns a wrapped integer or panics.
    pub fn unwrap_integer(&self) -> isize {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => {
                panic!("ObjectValue::unwrap_integer() called on a non integer");
            }
        }
    }

    /// Returns a wrapped thread or panics.
    pub fn unwrap_thread(&self) -> RcThread {
        match *self {
            ObjectValue::Thread(ref val) => val.clone(),
            _ => {
                panic!("ObjectValue::unwrap_thread() called on a non thread");
            }
        }
    }
}

/// A mutable, reference counted Object.
pub type RcObject = Arc<Object>;

/// Structure for storing information about a single Object.
pub struct Object {
    /// A unique ID associated with the object.
    pub id: RwLock<usize>,

    /// The name of the object, used in error messages if present.
    pub name: RwLock<Option<String>>,

    /// The prototype of the object.
    pub prototype: RwLock<Option<RcObject>>,

    /// The attributes of the object.
    pub attributes: RwLock<HashMap<String, RcObject>>,

    /// The constants defined in this object.
    pub constants: RwLock<HashMap<String, RcObject>>,

    /// The methods defined on this object.
    pub methods: RwLock<HashMap<String, RcCompiledCode>>,

    /// A value associated with the object, if any.
    // TODO: use something like a pointer so Object isn't super fat size wise
    pub value: RwLock<ObjectValue>,

    /// When set to "true" this object won't be GC'd.
    pub pinned: RwLock<bool>
}

impl Object {
    /// Creates a new Object
    pub fn new(id: usize, value: ObjectValue) -> RcObject {
        let obj = Object {
            id: RwLock::new(id),
            name: RwLock::new(None),
            prototype: RwLock::new(None),
            attributes: RwLock::new(HashMap::new()),
            constants: RwLock::new(HashMap::new()),
            methods: RwLock::new(HashMap::new()),
            value: RwLock::new(value),
            pinned: RwLock::new(false)
        };

        Arc::new(obj)
    }

    /// Returns the ID of this object.
    pub fn id(&self) -> usize {
        *self.id.read().unwrap()
    }

    /// Sets the name of this object.
    pub fn set_name(&self, name: String) {
        let mut self_name = self.name.write().unwrap();

        *self_name = Some(name);
    }

    /// Sets the prototype of this object.
    pub fn set_prototype(&self, prototype: RcObject) {
        let mut proto = self.prototype.write().unwrap();

        *proto = Some(prototype);
    }

    /// Pins the current object.
    pub fn pin(&self) {
        let mut pinned = self.pinned.write().unwrap();

        *pinned = true;
    }

    /// Unpins the current object.
    pub fn unpin(&self) {
        let mut pinned = self.pinned.write().unwrap();

        *pinned = false;
    }

    /// Returns an error message for undefined method calls.
    pub fn undefined_method_error(&self, name: &String) -> String {
        let proto    = self.prototype.read().unwrap();
        let obj_name = self.name.read().unwrap();

        if obj_name.is_some() {
            format!(
                "Undefined method \"{}\" called on a {}",
                name,
                obj_name.as_ref().unwrap()
            )
        }
        else if proto.is_some() {
            let proto_unwrapped = proto.as_ref().unwrap();

            proto_unwrapped.undefined_method_error(name)
        }
        else {
            format!("Undefined method \"{}\" called", name)
        }
    }

    /// Returns an error message for private method calls.
    pub fn private_method_error(&self, name: &String) -> String {
        let proto    = self.prototype.read().unwrap();
        let obj_name = self.name.read().unwrap();

        if obj_name.is_some() {
            format!(
                "Private method \"{}\" called on a {}",
                name,
                obj_name.as_ref().unwrap()
            )
        }
        else if proto.is_some() {
            let proto_unwrapped = proto.as_ref().unwrap();

            proto_unwrapped.private_method_error(name)
        }
        else {
            format!("Private method \"{}\" called", name)
        }
    }

    /// Adds a new method.
    pub fn add_method(&self, name: String, code: RcCompiledCode) {
        let mut methods = self.methods.write().unwrap();

        methods.insert(name, code.clone());
    }

    /// Looks up the method for the given name.
    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let mut retval: Option<RcCompiledCode> = None;

        let methods = self.methods.read().unwrap();
        let proto   = self.prototype.read().unwrap();

        // Method defined directly on the object
        if methods.contains_key(name) {
            retval = methods.get(name).cloned();
        }

        // Method defined somewhere in the object hierarchy
        else if proto.is_some() {
            let mut parent = proto.clone();

            while parent.is_some() {
                let unwrapped      = parent.unwrap();
                let parent_methods = unwrapped.methods.read().unwrap();

                if parent_methods.contains_key(name) {
                    retval = parent_methods.get(name).cloned();

                    break;
                }

                parent = unwrapped.prototype.read().unwrap().clone();
            }
        }

        retval
    }

    /// Adds a constant.
    pub fn add_constant(&self, name: String, value: RcObject) {
        let mut constants = self.constants.write().unwrap();

        constants.insert(name, value);
    }

    /// Adds a constant with the same name as the object.
    pub fn add_named_constant(&self, value: RcObject) {
        let name = value.name.read().unwrap().clone().unwrap();

        self.add_constant(name, value);
    }

    /// Looks up a constant in the current or a parent object.
    pub fn lookup_constant(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        let constants = self.constants.read().unwrap();
        let proto     = self.prototype.read().unwrap();

        if constants.contains_key(name) {
            retval = constants.get(name).cloned();
        }

        // Look up the constant in one of the parents.
        else if proto.is_some() {
            let mut parent = proto.clone();

            while parent.is_some() {
                let unwrapped        = parent.unwrap();
                let parent_constants = unwrapped.constants.read().unwrap();

                if parent_constants.contains_key(name) {
                    retval = parent_constants.get(name).cloned();

                    break;
                }

                parent = unwrapped.prototype.read().unwrap().clone();
            }
        }

        retval
    }

    /// Adds a new attribute to the object.
    pub fn add_attribute(&self, name: String, object: RcObject) {
        let mut attributes = self.attributes.write().unwrap();

        attributes.insert(name, object);
    }

    /// Returns the attribute for the given name.
    pub fn lookup_attribute(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        let proto      = self.prototype.read().unwrap();
        let attributes = self.attributes.read().unwrap();

        if attributes.contains_key(name) {
            retval = attributes.get(name).cloned();
        }

        else if proto.is_some() {
            let mut parent = proto.clone();

            while parent.is_some() {
                let unwrapped         = parent.unwrap();
                let parent_attributes = unwrapped.attributes.read().unwrap();

                if parent_attributes.contains_key(name) {
                    retval = parent_attributes.get(name).cloned();

                    break;
                }

                parent = unwrapped.prototype.read().unwrap().clone();
            }
        }

        retval
    }
}
