//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add attributes, etc.
use fnv::FnvHashMap;
use std::ops::Drop;
use std::ptr;

use gc::work_list::WorkList;
use object_pointer::{ObjectPointer, RawObjectPointer};
use object_value::ObjectValue;
use tagged_pointer::TaggedPointer;

macro_rules! push_collection {
    ($map:expr, $what:ident, $vec:expr) => {{
        $vec.reserve($map.len());

        for thing in $map.$what() {
            $vec.push(*thing);
        }
    }};
}

/// The status of an object.
#[derive(Eq, PartialEq, Debug)]
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

    /// This object is in the process of being moved.
    PendingMove,
}

pub type AttributesMap = FnvHashMap<ObjectPointer, ObjectPointer>;

/// The bit to set for objects that are being forwarded.
pub const PENDING_FORWARD_BIT: usize = 0;

/// The bit to set for objects that have been forwarded.
pub const FORWARDED_BIT: usize = 1;

/// The mask to apply when installing a forwarding pointer.
pub const FORWARDING_MASK: usize = 0x3;

/// Structure containing data of a single object.
pub struct Object {
    /// The prototype of this object.
    pub prototype: ObjectPointer,

    /// A pointer to the attributes of this object. Attributes are allocated
    /// on-demand and default to a NULL pointer.
    ///
    /// This pointer may be tagged to store extra information. The following
    /// bits can be set:
    ///
    /// * 00: this field contains a regular pointer.
    /// * 01: this object is in the process of being forwarded.
    /// * 10: this object has been forwarded, and this field is set to the
    ///   target object.
    pub attributes: TaggedPointer<AttributesMap>,

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
            attributes: TaggedPointer::null(),
            value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(
        value: ObjectValue,
        prototype: ObjectPointer,
    ) -> Object {
        Object {
            prototype,
            attributes: TaggedPointer::null(),
            value,
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

    /// Returns and removes the prototype of this object.
    pub fn take_prototype(&mut self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            let proto = self.prototype;

            self.prototype = ObjectPointer::null();

            Some(proto)
        }
    }

    /// Removes an attribute and returns it.
    pub fn remove_attribute(
        &mut self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map_mut() {
            map.remove(&name)
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
    pub fn lookup_attribute(
        &self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        let got = self.lookup_attribute_in_self(name);

        if got.is_some() {
            return got;
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype().is_some() {
            let mut opt_parent = self.prototype();

            while let Some(parent_ptr) = opt_parent {
                let parent = parent_ptr.get();
                let got = parent.lookup_attribute_in_self(name);

                if got.is_some() {
                    return got;
                }

                opt_parent = parent.prototype();
            }
        }

        None
    }

    /// Adds a new attribute to the current object.
    pub fn add_attribute(
        &mut self,
        name: ObjectPointer,
        object: ObjectPointer,
    ) {
        self.allocate_attributes_map();

        self.attributes_map_mut().unwrap().insert(name, object);
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map() {
            map.get(&name).cloned()
        } else {
            None
        }
    }

    /// Returns an immutable reference to the attributes.
    pub fn attributes_map(&self) -> Option<&AttributesMap> {
        self.attributes.as_ref()
    }

    pub fn attributes_map_mut(&self) -> Option<&mut AttributesMap> {
        self.attributes.as_mut()
    }

    pub fn set_attributes_map(&mut self, attrs: AttributesMap) {
        self.attributes = TaggedPointer::new(Box::into_raw(Box::new(attrs)));
    }

    /// Pushes all pointers in this object into the given Vec.
    pub fn push_pointers(&self, pointers: &mut WorkList) {
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
                if let Some(captures_from) = block.captures_from.as_ref() {
                    captures_from.push_pointers(pointers);
                }

                pointers.push(block.receiver.pointer());
            }
            ObjectValue::Binding(ref binding) => {
                binding.push_pointers(pointers);
            }
            _ => {}
        }
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj =
            Object::with_prototype(self.value.take(), self.prototype);

        if let Some(attributes) = self.take_attributes() {
            new_obj.attributes = attributes;
        }

        new_obj
    }

    pub fn take_attributes(&mut self) -> Option<TaggedPointer<AttributesMap>> {
        if !self.has_attributes() {
            return None;
        }

        let attrs = self.attributes.without_tags();

        // When the object is being forwarded we don't want to lose this status
        // by just setting the attributes to NULL. Doing so could result in
        // another collector thread to try and move the same object.
        self.attributes =
            TaggedPointer::with_bit(0x0 as _, PENDING_FORWARD_BIT);

        Some(attrs)
    }

    /// Tries to mark this object as pending a forward.
    ///
    /// This method returns true if forwarding is necessary, false otherwise.
    pub fn mark_for_forward(&mut self) -> bool {
        // This _must_ be a reference, otherwise we'll be operating on a _copy_
        // of the pointer, since TaggedPointer is a Copy type.
        let current = &mut self.attributes;
        let current_raw = current.raw;

        if current.atomic_bit_is_set(PENDING_FORWARD_BIT) {
            // Another thread is in the process of forwarding this object, or
            // just finished forwarding it (since forward_to() sets both bits).
            return false;
        }

        let desired =
            TaggedPointer::with_bit(current_raw, PENDING_FORWARD_BIT).raw;

        current.compare_and_swap(current_raw, desired)
    }

    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        // We use a mask that sets the lower 2 bits, instead of only setting
        // one. This removes the need for checking both bits when determining if
        // forwarding is necessary.
        let new_attrs =
            (pointer.raw.raw as usize | FORWARDING_MASK) as *mut AttributesMap;

        self.attributes.atomic_store(new_attrs);
    }

    /// Returns true if this object is forwarded.
    pub fn is_forwarded(&self) -> bool {
        self.attributes.atomic_bit_is_set(FORWARDED_BIT)
    }

    /// Returns true if this object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_attributes()
    }

    /// Returns true if an attributes map has been allocated.
    pub fn has_attributes(&self) -> bool {
        !self.attributes.is_null() && !self.is_forwarded()
    }

    pub fn drop_attributes(&mut self) {
        if let Some(attributes) = self.take_attributes() {
            drop(unsafe { Box::from_raw(attributes.untagged()) });
        }
    }

    pub fn write_to(self, raw_pointer: RawObjectPointer) -> ObjectPointer {
        unsafe {
            ptr::write(raw_pointer, self);
        }

        let pointer = ObjectPointer::new(raw_pointer);

        if pointer.is_finalizable() {
            pointer.mark_for_finalization();
        }

        pointer
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
        self.drop_attributes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use immix::block::Block;
    use object_pointer::{ObjectPointer, RawObjectPointer};
    use object_value::ObjectValue;
    use std::mem;

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
    fn test_object_take_prototype() {
        let mut obj = new_object();

        obj.set_prototype(fake_pointer());

        assert!(obj.prototype().is_some());

        let proto = obj.take_prototype();

        assert!(proto.is_some());
        assert!(obj.prototype().is_none());
    }

    #[test]
    fn test_object_remove_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name, fake_pointer());

        let attr = obj.remove_attribute(name);

        assert!(attr.is_some());
        assert!(obj.lookup_attribute(name).is_none());
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
    fn test_object_lookup_attribute_defined_in_receiver() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_defined_in_prototype() {
        let mut proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        proto.add_attribute(name.clone(), fake_pointer());
        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_with_prototype_without_attribute() {
        let proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute(name).is_none());
    }

    #[test]
    fn test_object_add_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_without_attribute() {
        let obj = new_object();
        let name = fake_pointer();

        assert!(obj.lookup_attribute(name).is_none());
    }

    #[test]
    fn test_object_lookup_attribute_with_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
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
        let mut pointers = WorkList::new();

        obj.push_pointers(&mut pointers);

        assert!(pointers.pop().is_none());
    }

    #[test]
    fn test_object_push_pointers_with_pointers() {
        let mut obj = new_object();
        let name = fake_pointer();
        let mut pointers = WorkList::new();

        obj.add_attribute(name, fake_pointer());

        obj.push_pointers(&mut pointers);

        assert!(pointers.pop().is_some());
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
    fn test_object_take_attributes() {
        let mut obj = Object::new(ObjectValue::Float(10.0));
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        let attr_opt = obj.take_attributes();

        assert!(attr_opt.is_some());
        assert!(obj.attributes.bit_is_set(PENDING_FORWARD_BIT));
        assert!(obj.attributes.is_null());

        unsafe {
            Box::from_raw(attr_opt.unwrap().untagged());
        }
    }

    #[test]
    fn test_object_forward_to() {
        let mut obj = new_object();
        let target = new_object();

        obj.forward_to(object_pointer_for(&target));

        assert!(obj.is_forwarded());
        assert!(!obj.attributes.is_null());
        assert!(object_pointer_for(&obj).is_forwarded());
    }

    #[test]
    fn test_object_size_of() {
        assert_eq!(mem::size_of::<Object>(), 32);
    }

    #[test]
    fn test_drop_attributes() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());
        obj.drop_attributes();

        assert!(obj.attributes_map().is_none());
    }

    #[test]
    fn test_object_write_to() {
        let mut block = Block::boxed();
        let raw_pointer = block.request_pointer().unwrap();
        let pointer = ObjectPointer::new(raw_pointer);

        Object::new(ObjectValue::Float(10.5)).write_to(raw_pointer);

        assert_eq!(pointer.float_value().unwrap(), 10.5);
    }

    #[test]
    fn test_object_mark_for_forward() {
        let mut block = Block::boxed();
        let object = Object::new(ObjectValue::None);

        // We have to explicitly write the object to the pointer, otherwise our
        // pointer might point to random chunk, which could result in it
        // thinking the pending forward bit is already set.
        let pointer = object.write_to(block.request_pointer().unwrap());

        assert!(pointer.get_mut().mark_for_forward());
        assert!(pointer.get().attributes.bit_is_set(PENDING_FORWARD_BIT));

        assert_eq!(pointer.get_mut().mark_for_forward(), false);
    }

    #[test]
    fn test_object_mark_for_forward_with_previously_forwarded_pointer() {
        let mut block = Block::boxed();
        let raw_pointer = block.request_pointer().unwrap();

        {
            let pointer = Object::new(ObjectValue::None).write_to(raw_pointer);

            pointer.get_mut().mark_for_forward();
        }

        // This test ensures that `mark_for_forward` doesn't get messed up when
        // used on a slot that was previously forwarded (but first allocated
        // into again).
        let pointer = Object::new(ObjectValue::None).write_to(raw_pointer);

        assert_eq!(
            pointer.get().attributes.bit_is_set(PENDING_FORWARD_BIT),
            false
        );

        assert_eq!(pointer.get_mut().mark_for_forward(), true);
    }
}
