//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.

use object_header::ObjectHeader;
use object_pointer::ObjectPointer;
use object_value::ObjectValue;

pub struct Object {
    pub prototype: Option<ObjectPointer>,
    pub header: Option<Box<ObjectHeader>>,
    pub value: ObjectValue
}

impl Object {
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: None,
            header: None,
            value: value,
        }
    }

    pub fn with_prototype(value: ObjectValue, proto: ObjectPointer) -> Object {
        Object {
            prototype: Some(proto),
            header: None,
            value: value
        }
    }

    pub fn set_prototype(&mut self, prototype: ObjectPointer) {
        self.prototype = Some(prototype);
    }

    pub fn prototype(&self) -> Option<ObjectPointer> {
        self.prototype.clone()
    }

    pub fn set_outer_scope(&mut self, scope: ObjectPointer) {
        self.allocate_header();

        let header_ref = self.header.as_mut().unwrap();

        header_ref.outer_scope = Some(scope);
    }

    pub fn add_method(&mut self, name: String, method: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header.as_mut().unwrap();

        header_ref.methods.insert(name, method);
    }

    pub fn responds_to(&self, name: &String) -> bool {
        self.lookup_method(name).is_some()
    }

    pub fn has_attribute(&self, name: &String) -> bool {
        self.lookup_attribute(name).is_some()
    }

    pub fn lookup_method(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header.as_ref();

        if let Some(header) = opt_header {
            // Method defined directly on the object
            if header.methods.contains_key(name) {
                return header.methods.get(name).cloned();
            }
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype.is_some() {
            let mut opt_parent = self.prototype.clone();

            while opt_parent.is_some() {
                let parent_ptr = opt_parent.unwrap();
                let parent_ref = parent_ptr.get();
                let parent     = parent_ref.get();

                let opt_parent_header = parent.header.as_ref();

                if opt_parent_header.is_some() {
                    let parent_header = opt_parent_header.unwrap();

                    if parent_header.methods.contains_key(name) {
                        retval = parent_header.methods.get(name).cloned();

                        break;
                    }
                }

                opt_parent = parent.prototype.clone();
            }
        }

        retval
    }

    pub fn add_constant(&mut self, name: String, value: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header.as_mut().unwrap();

        header_ref.constants.insert(name, value);
    }

    pub fn lookup_constant(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header.as_ref();

        if let Some(header) = opt_header {
            if header.constants.contains_key(name) {
                return header.constants.get(name).cloned();
            }
        }

        // Look up the constant in one of the parents.
        if let Some(proto) = self.prototype.as_ref() {
            let proto_ref = proto.get();

            retval = proto_ref.get().lookup_constant(name);
        }

        if retval.is_none() {
            if let Some(header) = opt_header {
                if let Some(scope) = header.outer_scope.as_ref() {
                    let scope_ref = scope.get();

                    retval = scope_ref.get().lookup_constant(name);
                }
            }
        }

        retval
    }

    pub fn add_attribute(&mut self, name: String, object: ObjectPointer) {
        self.allocate_header();

        let header = self.header.as_mut().unwrap();

        header.attributes.insert(name, object.clone());
    }

    pub fn lookup_attribute(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header.as_ref();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.unwrap();

        if header.attributes.contains_key(name) {
            retval = header.attributes.get(name).cloned();
        }

        retval
    }

    fn allocate_header(&mut self) {
        if self.header.is_none() {
            self.header = Some(Box::new(ObjectHeader::new()));
        }
    }
}
