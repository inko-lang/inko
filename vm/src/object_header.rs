//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods. An ObjectHeader struct is only
//! allocated when actually needed.
use std::collections::HashMap;

use immix::copy_object::CopyObject;
use object_pointer::ObjectPointer;

pub struct ObjectHeader {
    pub attributes: HashMap<String, ObjectPointer>,
    pub constants: HashMap<String, ObjectPointer>,
    pub methods: HashMap<String, ObjectPointer>,

    /// The object to use for constant lookups when a constant is not available
    /// in the prototype hierarchy.
    pub outer_scope: Option<ObjectPointer>,
}

impl ObjectHeader {
    pub fn new() -> ObjectHeader {
        ObjectHeader {
            attributes: HashMap::new(),
            constants: HashMap::new(),
            methods: HashMap::new(),
            outer_scope: None,
        }
    }

    pub fn pointers(&self) -> Vec<*const ObjectPointer> {
        let mut pointers = Vec::new();

        for (_, pointer) in self.attributes.iter() {
            pointers.push(pointer as *const ObjectPointer);
        }

        for (_, pointer) in self.constants.iter() {
            pointers.push(pointer as *const ObjectPointer);
        }

        for (_, pointer) in self.methods.iter() {
            pointers.push(pointer as *const ObjectPointer);
        }

        if let Some(scope) = self.outer_scope.as_ref() {
            pointers.push(scope as *const ObjectPointer);
        }

        pointers
    }

    pub fn copy_to<T: CopyObject>(&self, allocator: &mut T) -> ObjectHeader {
        let mut copy = ObjectHeader::new();

        for (key, value) in self.attributes.iter() {
            let value_copy = allocator.copy_object(value.clone());

            copy.add_attribute(key.clone(), value_copy);
        }

        for (key, value) in self.constants.iter() {
            let value_copy = allocator.copy_object(value.clone());

            copy.add_constant(key.clone(), value_copy);
        }

        for (key, value) in self.methods.iter() {
            let value_copy = allocator.copy_object(value.clone());

            copy.add_method(key.clone(), value_copy);
        }

        if let Some(scope) = self.outer_scope.as_ref() {
            let outer_copy = allocator.copy_object(scope.clone());

            copy.outer_scope = Some(outer_copy);
        }

        copy
    }

    pub fn add_method(&mut self, key: String, value: ObjectPointer) {
        self.methods.insert(key, value);
    }

    pub fn add_attribute(&mut self, key: String, value: ObjectPointer) {
        self.attributes.insert(key, value);
    }

    pub fn add_constant(&mut self, key: String, value: ObjectPointer) {
        self.constants.insert(key, value);
    }

    pub fn get_method(&self, key: &str) -> Option<ObjectPointer> {
        self.methods.get(key).cloned()
    }

    pub fn get_attribute(&self, key: &str) -> Option<ObjectPointer> {
        self.attributes.get(key).cloned()
    }

    pub fn get_constant(&self, key: &str) -> Option<ObjectPointer> {
        self.constants.get(key).cloned()
    }

    pub fn has_method(&self, key: &str) -> bool {
        self.methods.contains_key(key)
    }

    pub fn has_constant(&self, key: &str) -> bool {
        self.constants.contains_key(key)
    }

    pub fn has_attribute(&self, key: &str) -> bool {
        self.attributes.contains_key(key)
    }
}
