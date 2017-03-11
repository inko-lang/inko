//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods.
use std::collections::HashMap;
use immix::copy_object::CopyObject;
use object_pointer::{ObjectPointer, ObjectPointerPointer};

pub struct ObjectHeader {
    /// The attributes defined in an object.
    pub attributes: HashMap<String, ObjectPointer>,

    /// The constants defined in an object.
    pub constants: HashMap<String, ObjectPointer>,

    /// The methods defined in an object.
    pub methods: HashMap<String, ObjectPointer>,
}

impl ObjectHeader {
    pub fn new() -> ObjectHeader {
        ObjectHeader {
            attributes: HashMap::new(),
            constants: HashMap::new(),
            methods: HashMap::new(),
        }
    }

    /// Pushes all pointers in this header into the given Vec.
    pub fn push_pointers(&self, pointers: &mut Vec<ObjectPointerPointer>) {
        for (_, pointer) in self.attributes.iter() {
            pointers.push(pointer.pointer());
        }

        for (_, pointer) in self.constants.iter() {
            pointers.push(pointer.pointer());
        }

        for (_, pointer) in self.methods.iter() {
            pointers.push(pointer.pointer());
        }
    }

    /// Copies all pointers in this header to the given allocator.
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

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{RawObjectPointer, ObjectPointer};

    #[test]
    fn test_new() {
        let header = ObjectHeader::new();

        assert_eq!(header.attributes.len(), 0);
        assert_eq!(header.constants.len(), 0);
        assert_eq!(header.methods.len(), 0);
    }

    #[test]
    fn test_push_pointers() {
        let mut header = ObjectHeader::new();
        let mut pointers = Vec::new();

        header.add_method("test".to_string(), ObjectPointer::null());
        header.add_attribute("test".to_string(), ObjectPointer::null());
        header.add_constant("test".to_string(), ObjectPointer::null());

        header.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 3);

        // Make sure that updating the pointers also updates those stored in the
        // header.
        for pointer_pointer in pointers {
            let mut pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(header.get_method("test").unwrap().raw.raw as usize, 0x4);
        assert_eq!(header.get_attribute("test").unwrap().raw.raw as usize, 0x4);
        assert_eq!(header.get_constant("test").unwrap().raw.raw as usize, 0x4);
    }

    #[test]
    fn test_add_method() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_method(name.clone(), ObjectPointer::null());

        assert!(header.get_method(&name).is_some());
    }

    #[test]
    fn test_get_method_without_method() {
        let header = ObjectHeader::new();

        assert!(header.get_method(&"test".to_string()).is_none());
    }

    #[test]
    fn test_has_method_without_method() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_method(&"test".to_string()), false);
    }

    #[test]
    fn test_has_method_with_method() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_method(name.clone(), ObjectPointer::null());
        assert!(header.has_method(&name));
    }

    #[test]
    fn test_add_attribute() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_attribute(name.clone(), ObjectPointer::null());

        assert!(header.get_attribute(&name).is_some());
    }

    #[test]
    fn test_get_attribute_without_attribute() {
        let header = ObjectHeader::new();

        assert!(header.get_attribute(&"test".to_string()).is_none());
    }

    #[test]
    fn test_has_attribute_without_attribute() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_attribute(&"test".to_string()), false);
    }

    #[test]
    fn test_has_attribute_with_attribute() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_attribute(name.clone(), ObjectPointer::null());
        assert!(header.has_attribute(&name));
    }

    #[test]
    fn test_add_constant() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_constant(name.clone(), ObjectPointer::null());

        assert!(header.get_constant(&name).is_some());
    }

    #[test]
    fn test_get_constant_without_constant() {
        let header = ObjectHeader::new();

        assert!(header.get_constant(&"test".to_string()).is_none());
    }

    #[test]
    fn test_has_constant_without_constant() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_constant(&"test".to_string()), false);
    }

    #[test]
    fn test_has_constant_with_constant() {
        let mut header = ObjectHeader::new();
        let name = "test".to_string();

        header.add_constant(name.clone(), ObjectPointer::null());
        assert!(header.has_constant(&name));
    }
}
