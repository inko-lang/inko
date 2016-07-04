//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.

use std::ptr;
use std::mem;

use object_header::ObjectHeader;
use object_pointer::ObjectPointer;
use object_value::ObjectValue;

pub struct Object {
    pub prototype: *const ObjectPointer,
    pub header: Option<Box<ObjectHeader>>,
    pub value: ObjectValue,
}

unsafe impl Sync for Object {}
unsafe impl Send for Object {}

impl Object {
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: ptr::null(),
            header: None,
            value: value,
        }
    }

    pub fn with_prototype(value: ObjectValue, proto: ObjectPointer) -> Object {
        Object {
            prototype: unsafe { mem::transmute(proto) },
            header: None,
            value: value,
        }
    }

    pub fn set_prototype(&mut self, prototype: ObjectPointer) {
        self.prototype = unsafe { mem::transmute(prototype) };
    }

    pub fn prototype(&self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            unsafe {
                let ptr: ObjectPointer = mem::transmute(self.prototype);

                Some(ptr)
            }
        }
    }

    pub fn set_outer_scope(&mut self, scope: ObjectPointer) {
        self.allocate_header();

        let header_ref = self.header.as_mut().unwrap();

        header_ref.outer_scope = Some(scope);
    }

    pub fn add_method(&mut self, name: String, method: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header.as_mut().unwrap();

        header_ref.add_method(name, method);
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
            if header.has_method(name) {
                return header.get_method(name);
            }
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype().is_some() {
            let mut opt_parent = self.prototype();

            while opt_parent.is_some() {
                let parent_ptr = opt_parent.unwrap();
                let parent = parent_ptr.get();

                let opt_parent_header = parent.header.as_ref();

                if opt_parent_header.is_some() {
                    let parent_header = opt_parent_header.unwrap();

                    if parent_header.has_method(name) {
                        retval = parent_header.get_method(name);

                        break;
                    }
                }

                opt_parent = parent.prototype();
            }
        }

        retval
    }

    pub fn add_constant(&mut self, name: String, value: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header.as_mut().unwrap();

        header_ref.add_constant(name, value);
    }

    pub fn lookup_constant(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header.as_ref();

        if let Some(header) = opt_header {
            if header.has_constant(name) {
                return header.get_constant(name);
            }
        }

        // Look up the constant in one of the parents.
        if let Some(proto) = self.prototype() {
            retval = proto.get().lookup_constant(name);
        }

        if retval.is_none() {
            if let Some(header) = opt_header {
                if let Some(scope) = header.outer_scope.as_ref() {
                    retval = scope.get().lookup_constant(name);
                }
            }
        }

        retval
    }

    pub fn add_attribute(&mut self, name: String, object: ObjectPointer) {
        self.allocate_header();

        let header = self.header.as_mut().unwrap();

        header.add_attribute(name, object.clone());
    }

    pub fn lookup_attribute(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header.as_ref();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.unwrap();

        if header.has_attribute(name) {
            retval = header.get_attribute(name);
        }

        retval
    }

    pub fn header(&self) -> Option<&Box<ObjectHeader>> {
        self.header.as_ref()
    }

    pub fn set_header(&mut self, header: ObjectHeader) {
        self.header = Some(Box::new(header));
    }

    fn allocate_header(&mut self) {
        if self.header.is_none() {
            self.header = Some(Box::new(ObjectHeader::new()));
        }
    }
}
