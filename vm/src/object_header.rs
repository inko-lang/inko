//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods.
use std::collections::HashMap;
use immix::copy_object::CopyObject;
use object_pointer::{ObjectPointer, ObjectPointerPointer};

macro_rules! push_collection {
    ($header: expr, $source: ident, $what: ident, $vec: expr) => ({
        $vec.reserve($header.$source.len());

        for thing in $header.$source.$what() {
            $vec.push(*thing);
        }
    })
}

pub struct ObjectHeader {
    /// The attributes defined in an object.
    pub attributes: HashMap<ObjectPointer, ObjectPointer>,

    /// The constants defined in an object.
    pub constants: HashMap<ObjectPointer, ObjectPointer>,

    /// The methods defined in an object.
    pub methods: HashMap<ObjectPointer, ObjectPointer>,
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
            let value_copy = allocator.copy_object(*value);

            copy.add_attribute(*key, value_copy);
        }

        for (key, value) in self.constants.iter() {
            let value_copy = allocator.copy_object(*value);

            copy.add_constant(*key, value_copy);
        }

        for (key, value) in self.methods.iter() {
            let value_copy = allocator.copy_object(*value);

            copy.add_method(*key, value_copy);
        }

        copy
    }

    pub fn add_method(&mut self, key: ObjectPointer, value: ObjectPointer) {
        self.methods.insert(key, value);
    }

    pub fn add_attribute(&mut self, key: ObjectPointer, value: ObjectPointer) {
        self.attributes.insert(key, value);
    }

    pub fn add_constant(&mut self, key: ObjectPointer, value: ObjectPointer) {
        self.constants.insert(key, value);
    }

    pub fn get_method(&self, key: &ObjectPointer) -> Option<ObjectPointer> {
        self.methods.get(key).cloned()
    }

    pub fn get_attribute(&self, key: &ObjectPointer) -> Option<ObjectPointer> {
        self.attributes.get(key).cloned()
    }

    pub fn get_constant(&self, key: &ObjectPointer) -> Option<ObjectPointer> {
        self.constants.get(key).cloned()
    }

    pub fn has_method(&self, key: &ObjectPointer) -> bool {
        self.methods.contains_key(key)
    }

    pub fn has_constant(&self, key: &ObjectPointer) -> bool {
        self.constants.contains_key(key)
    }

    pub fn has_attribute(&self, key: &ObjectPointer) -> bool {
        self.attributes.contains_key(key)
    }

    pub fn remove_method(&mut self,
                         key: &ObjectPointer)
                         -> Option<ObjectPointer> {
        self.methods.remove(key)
    }

    pub fn remove_attribute(&mut self,
                            key: &ObjectPointer)
                            -> Option<ObjectPointer> {
        self.attributes.remove(key)
    }

    pub fn push_methods(&self, vec: &mut Vec<ObjectPointer>) {
        push_collection!(self, methods, values, vec);
    }

    pub fn push_method_names(&self, vec: &mut Vec<ObjectPointer>) {
        push_collection!(self, methods, keys, vec);
    }

    pub fn push_attributes(&self, vec: &mut Vec<ObjectPointer>) {
        push_collection!(self, attributes, values, vec);
    }

    pub fn push_attribute_names(&self, vec: &mut Vec<ObjectPointer>) {
        push_collection!(self, attributes, keys, vec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{RawObjectPointer, ObjectPointer};

    fn fake_pointer() -> ObjectPointer {
        ObjectPointer::new(0x1 as RawObjectPointer)
    }

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
        let name = fake_pointer();

        header.add_method(name, ObjectPointer::null());
        header.add_attribute(name, ObjectPointer::null());
        header.add_constant(name, ObjectPointer::null());

        header.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 3);

        // Make sure that updating the pointers also updates those stored in the
        // header.
        for pointer_pointer in pointers {
            let mut pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(header.get_method(&name).unwrap().raw.raw as usize, 0x4);
        assert_eq!(header.get_attribute(&name).unwrap().raw.raw as usize, 0x4);
        assert_eq!(header.get_constant(&name).unwrap().raw.raw as usize, 0x4);
    }

    #[test]
    fn test_add_method() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_method(name, ObjectPointer::null());

        assert!(header.get_method(&name).is_some());
    }

    #[test]
    fn test_get_method_without_method() {
        let header = ObjectHeader::new();

        assert!(header.get_method(&fake_pointer()).is_none());
    }

    #[test]
    fn test_has_method_without_method() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_method(&fake_pointer()), false);
    }

    #[test]
    fn test_has_method_with_method() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_method(name, ObjectPointer::null());
        assert!(header.has_method(&name));
    }

    #[test]
    fn test_add_attribute() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_attribute(name, ObjectPointer::null());

        assert!(header.get_attribute(&name).is_some());
    }

    #[test]
    fn test_get_attribute_without_attribute() {
        let header = ObjectHeader::new();

        assert!(header.get_attribute(&fake_pointer()).is_none());
    }

    #[test]
    fn test_has_attribute_without_attribute() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_attribute(&fake_pointer()), false);
    }

    #[test]
    fn test_has_attribute_with_attribute() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_attribute(name, ObjectPointer::null());
        assert!(header.has_attribute(&name));
    }

    #[test]
    fn test_add_constant() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_constant(name, ObjectPointer::null());

        assert!(header.get_constant(&name).is_some());
    }

    #[test]
    fn test_get_constant_without_constant() {
        let header = ObjectHeader::new();

        assert!(header.get_constant(&fake_pointer()).is_none());
    }

    #[test]
    fn test_has_constant_without_constant() {
        let header = ObjectHeader::new();

        assert_eq!(header.has_constant(&fake_pointer()), false);
    }

    #[test]
    fn test_has_constant_with_constant() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_constant(name, ObjectPointer::null());
        assert!(header.has_constant(&name));
    }

    #[test]
    fn test_remove_method() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_method(name, ObjectPointer::null());

        let method = header.remove_method(&name);

        assert!(method.is_some());
        assert!(header.get_method(&name).is_none());
    }

    #[test]
    fn test_remove_attribute() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();

        header.add_attribute(name, ObjectPointer::null());

        let attr = header.remove_attribute(&name);

        assert!(attr.is_some());
        assert!(header.get_attribute(&name).is_none());
    }

    #[test]
    fn test_push_methods() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();
        let method = ObjectPointer::null();
        let mut methods = Vec::new();

        header.add_method(name, method);
        header.push_methods(&mut methods);

        assert_eq!(methods.len(), 1);
        assert!(methods[0] == method);
    }

    #[test]
    fn test_push_method_names() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();
        let method = ObjectPointer::null();
        let mut names = Vec::new();

        header.add_method(name, method);
        header.push_method_names(&mut names);

        assert_eq!(names.len(), 1);
        assert!(names[0] == name);
    }

    #[test]
    fn test_push_attributes() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();
        let attribute = ObjectPointer::null();
        let mut attributes = Vec::new();

        header.add_attribute(name, attribute);
        header.push_attributes(&mut attributes);

        assert_eq!(attributes.len(), 1);
        assert!(attributes[0] == attribute);
    }

    #[test]
    fn test_push_attribute_names() {
        let mut header = ObjectHeader::new();
        let name = fake_pointer();
        let attribute = ObjectPointer::null();
        let mut names = Vec::new();

        header.add_attribute(name, attribute);
        header.push_attribute_names(&mut names);

        assert_eq!(names.len(), 1);
        assert!(names[0] == name);
    }
}
