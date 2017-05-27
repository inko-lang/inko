//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add attributes, etc.
use fnv::FnvHashMap;

use std::ops::Drop;
use std::ptr;

use object_pointer::{ObjectPointer, ObjectPointerPointer};
use object_value::ObjectValue;

macro_rules! push_collection {
    ($map: expr, $what: ident, $vec: expr) => ({
        $vec.reserve($map.len());

        for thing in $map.$what() {
            $vec.push(*thing);
        }
    })
}

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

pub type AttributesMap = FnvHashMap<ObjectPointer, ObjectPointer>;

/// Structure containing data of a single object.
pub struct Object {
    /// The prototype of this object.
    ///
    /// This pointer may be tagged to store extra information. The following
    /// bits can be set:
    ///
    ///     00: this field contains a regular pointer
    ///     10: this field contains a forwarding pointer
    pub prototype: ObjectPointer,

    /// A pointer to the attributes of this object. Attributes are allocated
    /// on-demand and default to a NULL pointer.
    pub attributes: *const AttributesMap,

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
            attributes: ptr::null::<AttributesMap>(),
            value: value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(value: ObjectValue, proto: ObjectPointer) -> Object {
        Object {
            prototype: proto,
            attributes: ptr::null::<AttributesMap>(),
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

    /// Removes an attribute and returns it.
    pub fn remove_attribute(&mut self,
                            name: &ObjectPointer)
                            -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map_mut() {
            map.remove(name)
        } else {
            None
        }
    }

    /// Returns all the attributes available to this object.
    pub fn attributes(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, values, attributes);
        }

        attributes
    }

    /// Returns all the attribute names available to this object.
    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, keys, attributes);
        }

        attributes
    }

    /// Looks up an attribute in either the current object or a parent object.
    pub fn lookup_attribute_chain(&self,
                                  name: &ObjectPointer)
                                  -> Option<ObjectPointer> {
        let got = self.lookup_attribute(name);

        if got.is_some() {
            return got;
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype().is_some() {
            let mut opt_parent = self.prototype();

            while let Some(parent_ptr) = opt_parent {
                let parent = parent_ptr.get();
                let got = parent.lookup_attribute(name);

                if got.is_some() {
                    return got;
                }

                opt_parent = parent.prototype();
            }
        }

        None
    }

    /// Adds a new attribute to the current object.
    pub fn add_attribute(&mut self, name: ObjectPointer, object: ObjectPointer) {
        self.allocate_attributes_map();

        self.attributes_map_mut().unwrap().insert(name, object);
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(&self,
                            name: &ObjectPointer)
                            -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map() {
            map.get(name).cloned()
        } else {
            None
        }
    }

    /// Returns an immutable reference to the attributes.
    pub fn attributes_map(&self) -> Option<&AttributesMap> {
        if self.attributes.is_null() {
            None
        } else {
            Some(unsafe { &*self.attributes })
        }
    }

    pub fn attributes_map_mut(&self) -> Option<&mut AttributesMap> {
        if self.attributes.is_null() {
            None
        } else {
            Some(unsafe { &mut *(self.attributes as *mut AttributesMap) })
        }
    }

    pub fn set_attributes_map(&mut self, attrs: AttributesMap) {
        self.attributes = Box::into_raw(Box::new(attrs));
    }

    /// Pushes all pointers in this object into the given Vec.
    pub fn push_pointers(&self, pointers: &mut Vec<ObjectPointerPointer>) {
        if !self.prototype.is_null() {
            pointers.push(self.prototype.pointer());
        }

        if let Some(map) = self.attributes_map() {
            // Attribute keys are interned strings, which don't need to be
            // marked.
            for (_, pointer) in map.iter() {
                pointers.push(pointer.pointer());
            }
        }

        match self.value {
            ObjectValue::Array(ref array) => {
                for pointer in array.iter() {
                    pointers.push(pointer.pointer());
                }
            }
            ObjectValue::Block(ref block) => {
                block.binding.push_pointers(pointers)
            }
            ObjectValue::Binding(ref binding) => binding.push_pointers(pointers),
            _ => {}
        }
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj = Object::with_prototype(self.value.take(),
                                                 self.prototype);

        new_obj.attributes = self.attributes;
        self.attributes = ptr::null::<AttributesMap>();

        new_obj
    }

    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        self.prototype = pointer.forwarding_pointer();
    }

    /// Returns true if this object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_attributes()
    }

    /// Returns true if an attributes map has been allocated.
    pub fn has_attributes(&self) -> bool {
        !self.attributes.is_null()
    }

    /// Allocates an attribute map if needed.
    fn allocate_attributes_map(&mut self) {
        if !self.has_attributes() {
            self.set_attributes_map(AttributesMap::default());
        }
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        if self.has_attributes() {
            drop(unsafe { Box::from_raw(self.attributes as *mut AttributesMap) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
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
        assert!(obj.attributes.is_null());
        assert!(obj.value.is_none());
    }

    #[test]
    fn test_object_with_prototype() {
        let obj = Object::with_prototype(ObjectValue::None, fake_pointer());

        assert_eq!(obj.prototype.is_null(), false);
        assert!(obj.attributes.is_null());
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
    fn test_object_remove_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name, fake_pointer());

        let attr = obj.remove_attribute(&name);

        assert!(attr.is_some());
        assert!(obj.lookup_attribute(&name).is_none());
    }

    #[test]
    fn test_object_attributes() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert_eq!(obj.attributes().len(), 1);
    }

    #[test]
    fn test_object_attribute_names() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert_eq!(obj.attribute_names().len(), 1);
    }

    #[test]
    fn test_object_lookup_attribute_chain() {
        let obj = new_object();

        assert!(obj.lookup_attribute_chain(&fake_pointer()).is_none());
    }

    #[test]
    fn test_object_lookup_attribute_chain_defined_in_receiver() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute_chain(&name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_chain_defined_in_prototype() {
        let mut proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        proto.add_attribute(name.clone(), fake_pointer());
        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute_chain(&name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_chain_with_prototype_without_method() {
        let proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute_chain(&name).is_none());
    }

    #[test]
    fn test_object_add_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(&name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_without_attribute() {
        let obj = new_object();
        let name = fake_pointer();

        assert!(obj.lookup_attribute(&name).is_none());
    }

    #[test]
    fn test_object_lookup_attribute_with_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(&name).is_some());
    }

    #[test]
    fn test_object_attributes_map_without_map() {
        let obj = new_object();

        assert!(obj.attributes_map().is_none());
    }

    #[test]
    fn test_object_attributes_map_with_map() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert!(obj.attributes_map().is_some());
        assert!(obj.attributes_map_mut().is_some());
    }

    #[test]
    fn test_object_attributes_map_set_map() {
        let mut obj = new_object();
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        assert!(obj.attributes_map().is_some());
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
        let name = fake_pointer();
        let mut pointers = Vec::new();

        obj.add_attribute(name, fake_pointer());

        obj.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 1);
    }

    #[test]
    fn test_object_take() {
        let mut obj = Object::new(ObjectValue::Float(10.0));
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        let new_obj = obj.take();

        assert!(obj.attributes_map().is_none());
        assert!(obj.value.is_none());

        assert!(new_obj.attributes_map().is_some());
        assert!(new_obj.value.is_float());
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
