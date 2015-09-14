//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.

use std::sync::{Arc, RwLock};

use compiled_code::RcCompiledCode;
use object_header::ObjectHeader;
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
    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _                       => false
        }
    }

    pub fn unwrap_integer(&self) -> isize {
        match *self {
            ObjectValue::Integer(val) => val,
            _ => {
                panic!("ObjectValue::unwrap_integer() called on a non integer");
            }
        }
    }

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
    pub id: usize,
    pub prototype: RwLock<Option<RcObject>>,
    pub header: RwLock<Option<Box<ObjectHeader>>>,

    // TODO: use something like a pointer so Object isn't super fat size wise
    pub value: RwLock<ObjectValue>,
}

impl Object {
    pub fn new(id: usize, value: ObjectValue) -> RcObject {
        let obj = Object {
            id: id,
            prototype: RwLock::new(None),
            header: RwLock::new(None),
            value: RwLock::new(value),
        };

        Arc::new(obj)
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_name(&self, name: String) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let header_ref     = opt_header.as_mut().unwrap();

        header_ref.name = Some(name);
    }

    pub fn set_prototype(&self, prototype: RcObject) {
        let mut proto = self.prototype.write().unwrap();

        *proto = Some(prototype);
    }

    pub fn pin(&self) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let header_ref     = opt_header.as_mut().unwrap();

        header_ref.pinned = true;
    }

    pub fn unpin(&self) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let header_ref     = opt_header.as_mut().unwrap();

        header_ref.pinned = false;
    }

    /// Returns an error message for undefined method calls.
    pub fn undefined_method_error(&self, name: &String) -> String {
        let proto      = self.prototype.read().unwrap();
        let opt_header = self.header.read().unwrap();

        let obj_name = if opt_header.is_some() {
            opt_header.as_ref().unwrap().name.as_ref()
        }
        else {
            None
        };

        if obj_name.is_some() {
            format!(
                "Undefined method \"{}\" called on a {}",
                name,
                obj_name.unwrap()
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
        let proto      = self.prototype.read().unwrap();
        let opt_header = self.header.read().unwrap();

        let obj_name = if opt_header.is_some() {
            opt_header.as_ref().unwrap().name.as_ref()
        }
        else {
            None
        };

        if obj_name.is_some() {
            format!(
                "Private method \"{}\" called on a {}",
                name,
                obj_name.unwrap()
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

    pub fn add_method(&self, name: String, code: RcCompiledCode) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let mut header_ref = opt_header.as_mut().unwrap();

        header_ref.methods.insert(name, code.clone());
    }

    pub fn lookup_method(&self, name: &String) -> Option<RcCompiledCode> {
        let mut retval: Option<RcCompiledCode> = None;

        let opt_header = self.header.read().unwrap();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.as_ref().unwrap();
        let proto  = self.prototype.read().unwrap();

        // Method defined directly on the object
        if header.methods.contains_key(name) {
            retval = header.methods.get(name).cloned();
        }

        // Method defined somewhere in the object hierarchy
        else if proto.is_some() {
            let mut opt_parent = proto.clone();

            while opt_parent.is_some() {
                let parent = opt_parent.unwrap();

                let opt_parent_header = parent.header.read().unwrap();

                if opt_parent_header.is_some() {
                    let parent_header = opt_parent_header.as_ref().unwrap();

                    if parent_header.methods.contains_key(name) {
                        retval = parent_header.methods.get(name).cloned();

                        break;
                    }
                }

                opt_parent = parent.prototype.read().unwrap().clone();
            }
        }

        retval
    }

    pub fn add_constant(&self, name: String, value: RcObject) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let mut header_ref = opt_header.as_mut().unwrap();

        header_ref.constants.insert(name, value);
    }

    /// Adds a constant with the same name as the object.
    pub fn add_named_constant(&self, value: RcObject) {
        let opt_header = value.header.read().unwrap();
        let header_ref = opt_header.as_ref().unwrap();

        let name = header_ref.name.clone().unwrap();

        self.add_constant(name, value.clone());
    }

    pub fn lookup_constant(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        let opt_header = self.header.read().unwrap();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.as_ref().unwrap();
        let proto  = self.prototype.read().unwrap();

        if header.constants.contains_key(name) {
            retval = header.constants.get(name).cloned();
        }

        // Look up the constant in one of the parents.
        else if proto.is_some() {
            let mut opt_parent = proto.clone();

            while opt_parent.is_some() {
                let parent = opt_parent.unwrap();

                let opt_parent_header = parent.header.read().unwrap();

                if opt_parent_header.is_some() {
                    let parent_header = opt_parent_header.as_ref().unwrap();

                    if parent_header.constants.contains_key(name) {
                        retval = parent_header.constants.get(name).cloned();

                        break;
                    }
                }

                opt_parent = parent.prototype.read().unwrap().clone();
            }
        }

        retval
    }

    pub fn add_attribute(&self, name: String, object: RcObject) {
        self.allocate_header();

        let mut opt_header = self.header.write().unwrap();
        let mut header_ref = opt_header.as_mut().unwrap();

        header_ref.attributes.insert(name, object.clone());
    }

    pub fn lookup_attribute(&self, name: &String) -> Option<RcObject> {
        let mut retval: Option<RcObject> = None;

        let opt_header = self.header.read().unwrap();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.as_ref().unwrap();

        if header.attributes.contains_key(name) {
            retval = header.attributes.get(name).cloned();
        }

        retval
    }

    fn allocate_header(&self) {
        let mut header = self.header.write().unwrap();

        if header.is_none() {
            *header = Some(Box::new(ObjectHeader::new()));
        }
    }
}
