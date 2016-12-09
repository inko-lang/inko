//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.
use std::ops::Drop;
use std::ptr;

use object_header::ObjectHeader;
use object_pointer::{ObjectPointer, ObjectPointerPointer};
use object_value::ObjectValue;

/// The status of an object.
pub enum ObjectStatus {
    /// This object is OK and no action has to be taken by a collector.
    OK,

    /// This object has been forwarded and all forwarding pointers must be
    /// resolved.
    Resolve,

    /// This object is ready to be promoted to the mature generation.
    Promote,

    /// This object should be evacuated from its block.
    Evacuate,
}

/// Structure containing data of a single object.
pub struct Object {
    /// The prototype of this object. Method and constant lookups use the
    /// prototype chain in case a method/constant couldn't be found in the
    /// current object.
    ///
    /// This pointer may be tagged to store extra information. The following
    /// bits can be set:
    ///
    ///     00: this field contains a regular pointer
    ///     10: this field contains a forwarding pointer
    pub prototype: ObjectPointer,

    /// A pointer to a header storing the methods, attributes, and other data of
    /// this object. Headers are allocated on demand and default to null
    /// pointers.
    pub header: *const ObjectHeader,

    /// A native Rust value (e.g. a String) that belongs to this object.
    pub value: ObjectValue,
}

unsafe impl Sync for Object {}
unsafe impl Send for Object {}

impl Object {
    /// Returns a new object with the given value.
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: ObjectPointer::null(),
            header: ptr::null::<ObjectHeader>(),
            value: value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(value: ObjectValue, proto: ObjectPointer) -> Object {
        Object {
            prototype: proto,
            header: ptr::null::<ObjectHeader>(),
            value: value,
        }
    }

    /// Sets the prototype of this object.
    pub fn set_prototype(&mut self, prototype: ObjectPointer) {
        self.prototype = prototype;
    }

    /// Returns the prototype of this object.
    pub fn prototype(&self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            Some(self.prototype)
        }
    }

    /// Sets the outer scope used for constant lookups.
    pub fn set_outer_scope(&mut self, scope: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header_mut().unwrap();

        header_ref.outer_scope = Some(scope);
    }

    /// Adds a new method to this object.
    pub fn add_method(&mut self, name: String, method: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header_mut().unwrap();

        header_ref.add_method(name, method);
    }

    /// Returns true if the object responds to the given message.
    pub fn responds_to(&self, name: &String) -> bool {
        self.lookup_method(name).is_some()
    }

    /// Returns true if the object has the given attribute.
    pub fn has_attribute(&self, name: &String) -> bool {
        self.lookup_attribute(name).is_some()
    }

    /// Looks up a method.
    pub fn lookup_method(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header();

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

                let opt_parent_header = parent.header();

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

    /// Adds a new constant to the current object.
    pub fn add_constant(&mut self, name: String, value: ObjectPointer) {
        self.allocate_header();

        let mut header_ref = self.header_mut().unwrap();

        header_ref.add_constant(name, value);
    }

    /// Looks up a constant.
    pub fn lookup_constant(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header();

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

    /// Adds a new attribute to the current object.
    pub fn add_attribute(&mut self, name: String, object: ObjectPointer) {
        self.allocate_header();

        let mut header = self.header_mut().unwrap();

        header.add_attribute(name, object.clone());
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(&self, name: &String) -> Option<ObjectPointer> {
        let mut retval: Option<ObjectPointer> = None;

        let opt_header = self.header();

        if opt_header.is_none() {
            return retval;
        }

        let header = opt_header.unwrap();

        if header.has_attribute(name) {
            retval = header.get_attribute(name);
        }

        retval
    }

    /// Returns an immutable reference to the object header.
    pub fn header(&self) -> Option<&ObjectHeader> {
        if self.header.is_null() {
            None
        } else {
            Some(unsafe { &*self.header })
        }
    }

    /// Returns a mutable reference to the object header.
    pub fn header_mut(&self) -> Option<&mut ObjectHeader> {
        if self.header.is_null() {
            None
        } else {
            Some(unsafe { &mut *(self.header as *mut ObjectHeader) })
        }
    }

    /// Sets the object header to the given header.
    pub fn set_header(&mut self, header: ObjectHeader) {
        self.header = Box::into_raw(Box::new(header));
    }

    /// Pushes all pointers in this object into the given Vec.
    pub fn push_pointers(&self, pointers: &mut Vec<ObjectPointerPointer>) {
        if !self.prototype.is_null() {
            pointers.push(self.prototype.pointer());
        }

        if let Some(header) = self.header() {
            header.push_pointers(pointers);
        }

        match self.value {
            ObjectValue::Array(ref array) => {
                for pointer in array.iter() {
                    pointers.push(pointer.pointer());
                }
            }
            ObjectValue::Binding(ref binding) => {
                binding.push_pointers(pointers);
            }
            _ => {}
        }
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj = Object::with_prototype(self.value.take(),
                                                 self.prototype);

        new_obj.header = self.header;
        self.header = ptr::null::<ObjectHeader>();

        new_obj
    }

    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        self.prototype = pointer.forwarding_pointer();
    }

    /// Returns true if this object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_header()
    }

    /// Returns true if an object header has been allocated.
    pub fn has_header(&self) -> bool {
        !self.header.is_null()
    }

    /// Allocates an object header if needed.
    fn allocate_header(&mut self) {
        if !self.has_header() {
            self.set_header(ObjectHeader::new());
        }
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        if self.has_header() {
            drop(unsafe { Box::from_raw(self.header as *mut ObjectHeader) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use object_header::ObjectHeader;
    use object_value::ObjectValue;
    use object_pointer::{ObjectPointer, RawObjectPointer};

    fn fake_pointer() -> ObjectPointer {
        ObjectPointer::new(0x4 as RawObjectPointer)
    }

    fn new_object() -> Object {
        Object::new(ObjectValue::None)
    }

    fn object_pointer_for(object: &Object) -> ObjectPointer {
        ObjectPointer::new(object as *const Object as RawObjectPointer)
    }

    #[test]
    fn test_object_new() {
        let obj = new_object();

        assert!(obj.prototype.is_null());
        assert!(obj.header.is_null());
        assert!(obj.value.is_none());
    }

    #[test]
    fn test_object_with_prototype() {
        let obj = Object::with_prototype(ObjectValue::None, fake_pointer());

        assert_eq!(obj.prototype.is_null(), false);
        assert!(obj.header.is_null());
    }

    #[test]
    fn test_object_set_prototype() {
        let mut obj = new_object();

        assert!(obj.prototype.is_null());

        obj.set_prototype(fake_pointer());

        assert_eq!(obj.prototype.is_null(), false);
    }

    #[test]
    fn test_object_prototype() {
        let mut obj = new_object();

        assert!(obj.prototype().is_none());

        obj.set_prototype(fake_pointer());

        assert!(obj.prototype().is_some());
    }

    #[test]
    fn test_object_set_outer_scope() {
        let mut obj = new_object();

        assert!(obj.header.is_null());

        obj.set_outer_scope(fake_pointer());

        assert!(obj.header().is_some());
        assert!(obj.header().unwrap().outer_scope.is_some());
    }

    #[test]
    fn test_object_add_method() {
        let mut obj = new_object();

        obj.add_method("test".to_string(), fake_pointer());

        assert!(obj.lookup_method(&"test".to_string()).is_some());
    }

    #[test]
    fn test_object_responds_to_without_method() {
        let obj = new_object();

        assert_eq!(obj.responds_to(&"test".to_string()), false);
    }

    #[test]
    fn test_object_responds_to_with_method() {
        let mut obj = new_object();

        obj.add_method("test".to_string(), fake_pointer());

        assert!(obj.responds_to(&"test".to_string()));
    }

    #[test]
    fn test_object_has_attribute_without_attribute() {
        let obj = new_object();

        assert_eq!(obj.has_attribute(&"test".to_string()), false);
    }

    #[test]
    fn test_object_has_attribute_with_attribute() {
        let mut obj = new_object();

        obj.add_attribute("test".to_string(), fake_pointer());

        assert!(obj.has_attribute(&"test".to_string()));
    }

    #[test]
    fn test_object_lookup_method() {
        let obj = new_object();

        assert!(obj.lookup_method(&"test".to_string()).is_none());
    }

    #[test]
    fn test_object_lookup_method_defined_in_receiver() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_method(name.clone(), fake_pointer());

        assert!(obj.lookup_method(&name).is_some());
    }

    #[test]
    fn test_object_lookup_method_defined_in_prototype() {
        let mut proto = new_object();
        let mut child = new_object();
        let name = "test".to_string();

        proto.add_method(name.clone(), fake_pointer());
        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_method(&name).is_some());
    }

    #[test]
    fn test_object_lookup_method_with_prototype_without_method() {
        let proto = new_object();
        let mut child = new_object();
        let name = "test".to_string();

        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_method(&name).is_none());
    }

    #[test]
    fn test_object_add_constant() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_constant(name.clone(), fake_pointer());

        assert!(obj.lookup_constant(&name).is_some());
    }

    #[test]
    fn test_object_lookup_constant_without_constant() {
        let obj = new_object();
        let name = "test".to_string();

        assert!(obj.lookup_constant(&name).is_none());
    }

    #[test]
    fn test_object_lookup_constant_with_constant_defined_in_receiver() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_constant(name.clone(), fake_pointer());

        assert!(obj.lookup_constant(&name).is_some());
    }

    #[test]
    fn test_object_lookup_constant_with_constant_defined_in_prototype() {
        let mut proto = new_object();
        let mut child = new_object();
        let name = "test".to_string();

        proto.add_constant(name.clone(), fake_pointer());
        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_constant(&name).is_some());
    }

    #[test]
    fn test_object_lookup_constant_with_constant_defined_in_outer_scope() {
        let mut outer_scope = new_object();
        let mut obj = new_object();
        let name = "test".to_string();

        outer_scope.add_constant(name.clone(), fake_pointer());
        obj.set_outer_scope(object_pointer_for(&outer_scope));

        assert!(obj.lookup_constant(&name).is_some());
    }

    #[test]
    fn test_object_add_attribute() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(&name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_without_attribute() {
        let obj = new_object();
        let name = "test".to_string();

        assert!(obj.lookup_attribute(&name).is_none());
    }

    #[test]
    fn test_object_lookup_attribute_with_attribute() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(&name).is_some());
    }

    #[test]
    fn test_object_header_without_header() {
        let obj = new_object();

        assert!(obj.header().is_none());
    }

    #[test]
    fn test_object_header_with_header() {
        let mut obj = new_object();

        obj.add_attribute("test".to_string(), fake_pointer());

        assert!(obj.header().is_some());
        assert!(obj.header_mut().is_some());
    }

    #[test]
    fn test_object_header_set_header() {
        let mut obj = new_object();
        let header = ObjectHeader::new();

        obj.set_header(header);

        assert!(obj.header().is_some());
    }

    #[test]
    fn test_object_push_pointers_without_pointers() {
        let obj = new_object();
        let mut pointers = Vec::new();

        obj.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 0);
    }

    #[test]
    fn test_object_push_pointers_with_pointers() {
        let mut obj = new_object();
        let name = "test".to_string();
        let mut pointers = Vec::new();

        obj.add_method(name.clone(), fake_pointer());
        obj.add_attribute(name.clone(), fake_pointer());
        obj.add_constant(name.clone(), fake_pointer());
        obj.set_outer_scope(fake_pointer());

        obj.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 4);
    }

    #[test]
    fn test_object_take() {
        let mut obj = Object::new(ObjectValue::Integer(10));
        let header = ObjectHeader::new();

        obj.set_header(header);

        let new_obj = obj.take();

        assert!(obj.header().is_none());
        assert!(obj.value.is_none());

        assert!(new_obj.header().is_some());
        assert!(new_obj.value.is_integer());
    }

    #[test]
    fn test_object_forward_to() {
        let mut obj = new_object();
        let target = new_object();

        obj.forward_to(object_pointer_for(&target));

        assert!(obj.prototype().is_some());
        assert!(object_pointer_for(&obj).is_forwarded());
    }

    #[test]
    fn test_object_size_of() {
        assert_eq!(mem::size_of::<Object>(), 32);
    }
}
