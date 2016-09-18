//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add constants, etc.

use object_header::ObjectHeader;
use object_pointer::ObjectPointer;
use object_value::ObjectValue;
use tagged_pointer::TaggedPointer;

/// The bit mask to use for the mature generation.
const MATURE_MASK: usize = 0x1;

/// The bit mask to use for the mailbox generation.
const MAILBOX_MASK: usize = 0x2;

/// The bit mask to use for the permanent generation.
const PERMANENT_MASK: usize = 0x3;

/// The generations an object can reside in.
pub enum ObjectGeneration {
    Young,
    Mature,
    Permanent,
    Mailbox,
}

impl ObjectGeneration {
    /// Returns true if the current generation is the permanent generation.
    pub fn is_permanent(&self) -> bool {
        match *self {
            ObjectGeneration::Permanent => true,
            _ => false,
        }
    }

    /// Returns true if the current generation is the mature generation.
    pub fn is_mature(&self) -> bool {
        match *self {
            ObjectGeneration::Mature => true,
            _ => false,
        }
    }

    /// Returns true if the current generation is the young generation.
    pub fn is_young(&self) -> bool {
        match *self {
            ObjectGeneration::Young => true,
            _ => false,
        }
    }

    /// Returns true if the current generation is the mailbox generation.
    pub fn is_mailbox(&self) -> bool {
        match *self {
            ObjectGeneration::Mailbox => true,
            _ => false,
        }
    }
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
    ///
    /// Header pointers are tagged so extra information can be encoded into an
    /// object without taking up extra space. Tagging is done by setting one of
    /// the lower two bits to 1. The possible values of these two bits are as
    /// follows:
    ///
    ///     00: object resides in the young generation
    ///     01: object resides in the mature generation
    ///     10: object resides in the mailbox generation
    ///     11: object resides in the permanent generation
    pub header: TaggedPointer<ObjectHeader>,

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
            header: TaggedPointer::null(),
            value: value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(value: ObjectValue, proto: ObjectPointer) -> Object {
        Object {
            prototype: proto,
            header: TaggedPointer::null(),
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
        self.header.as_ref()
    }

    /// Returns a mutable reference to the object header.
    pub fn header_mut(&self) -> Option<&mut ObjectHeader> {
        self.header.as_mut()
    }

    /// Sets the object header to the given header.
    pub fn set_header(&mut self, header: ObjectHeader) {
        let header = Box::new(header);
        let pointer = Box::into_raw(header);

        self.header = match self.generation() {
            ObjectGeneration::Young => TaggedPointer::new(pointer),
            ObjectGeneration::Mature => {
                TaggedPointer::with_mask(pointer, MATURE_MASK)
            }
            ObjectGeneration::Permanent => {
                TaggedPointer::with_mask(pointer, PERMANENT_MASK)
            }
            ObjectGeneration::Mailbox => {
                TaggedPointer::with_mask(pointer, MAILBOX_MASK)
            }
        };
    }

    /// Deallocates any pointers stored directly in this object.
    ///
    /// Drop adds a flag which increases the struct size. To work around this
    /// we use this drop-like method that's explicitly called by the garbage
    /// collector.
    pub fn deallocate_pointers(&mut self) {
        if !self.header.is_null() {
            let boxed = unsafe { Box::from_raw(self.header.untagged()) };

            drop(boxed);
        }
    }

    /// Sets the generation of this object to the permanent generation.
    pub fn set_permanent(&mut self) {
        self.header.set_mask(PERMANENT_MASK);
    }

    /// Sets the generation of this object to the mature generation.
    pub fn set_mature(&mut self) {
        self.header.set_mask(MATURE_MASK);
    }

    /// Sets the generation of this object to the mailbox generation.
    pub fn set_mailbox(&mut self) {
        self.header.set_mask(MAILBOX_MASK);
    }

    /// Returns the generation this object belongs to.
    pub fn generation(&self) -> ObjectGeneration {
        // Due to the bit masks used we must compare in the order of greatest to
        // smallest bit mask.
        if self.header.mask_is_set(PERMANENT_MASK) {
            ObjectGeneration::Permanent
        } else if self.header.mask_is_set(MAILBOX_MASK) {
            ObjectGeneration::Mailbox
        } else if self.header.mask_is_set(MATURE_MASK) {
            ObjectGeneration::Mature
        } else {
            ObjectGeneration::Young
        }
    }

    /// Returns all the pointers stored in this object.
    pub fn pointers(&self) -> Vec<*const ObjectPointer> {
        let mut pointers = Vec::new();

        if !self.prototype.is_null() {
            pointers.push(&self.prototype as *const ObjectPointer);
        }

        if let Some(header) = self.header() {
            let mut header_pointers = header.pointers();

            pointers.append(&mut header_pointers);
        }

        pointers
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj = Object::with_prototype(self.value.take(),
                                                 self.prototype);

        new_obj.header = self.header;
        self.header = TaggedPointer::null();

        new_obj
    }

    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        self.prototype = pointer.forwarding_pointer();
    }

    /// Allocates an object header if needed.
    fn allocate_header(&mut self) {
        if self.header.is_null() {
            self.set_header(ObjectHeader::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_object_generation_is_permanent() {
        assert_eq!(ObjectGeneration::Young.is_permanent(), false);
        assert_eq!(ObjectGeneration::Mature.is_permanent(), false);
        assert_eq!(ObjectGeneration::Permanent.is_permanent(), true);
        assert_eq!(ObjectGeneration::Mailbox.is_permanent(), false);
    }

    #[test]
    fn test_object_generation_is_mature() {
        assert_eq!(ObjectGeneration::Young.is_mature(), false);
        assert_eq!(ObjectGeneration::Mature.is_mature(), true);
        assert_eq!(ObjectGeneration::Permanent.is_mature(), false);
        assert_eq!(ObjectGeneration::Mailbox.is_mature(), false);
    }

    #[test]
    fn test_object_generation_is_young() {
        assert_eq!(ObjectGeneration::Young.is_young(), true);
        assert_eq!(ObjectGeneration::Mature.is_young(), false);
        assert_eq!(ObjectGeneration::Permanent.is_young(), false);
        assert_eq!(ObjectGeneration::Mailbox.is_young(), false);
    }

    #[test]
    fn test_object_generation_is_mailbox() {
        assert_eq!(ObjectGeneration::Young.is_mailbox(), false);
        assert_eq!(ObjectGeneration::Mature.is_mailbox(), false);
        assert_eq!(ObjectGeneration::Permanent.is_mailbox(), false);
        assert_eq!(ObjectGeneration::Mailbox.is_mailbox(), true);
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
    fn test_object_header_set_header_with_generation() {
        let mut obj = new_object();
        let header = ObjectHeader::new();

        obj.set_permanent();
        obj.set_header(header);

        assert!(obj.generation().is_permanent());
    }

    #[test]
    fn test_object_deallocate_pointers() {
        let mut obj = new_object();

        obj.deallocate_pointers();
    }

    #[test]
    fn test_object_deallocate_pointers_with_header() {
        let mut obj = new_object();
        let header = ObjectHeader::new();

        obj.set_header(header);
        obj.deallocate_pointers();
    }

    #[test]
    fn test_object_set_permanent() {
        let mut obj = new_object();

        obj.set_permanent();

        assert!(obj.generation().is_permanent());
    }

    #[test]
    fn test_object_set_mature() {
        let mut obj = new_object();

        obj.set_mature();

        assert!(obj.generation().is_mature());
    }

    #[test]
    fn test_object_generation_with_default_generation() {
        let obj = new_object();

        assert!(obj.generation().is_young());
    }

    #[test]
    fn test_object_generation_with_permanent_generation() {
        let mut obj = new_object();

        obj.set_permanent();

        assert!(obj.generation().is_permanent());
    }

    #[test]
    fn test_object_generation_with_mailbox_generation() {
        let mut obj = new_object();

        obj.set_mailbox();

        assert!(obj.generation().is_mailbox());
    }

    #[test]
    fn test_object_generation_with_mature_generation() {
        let mut obj = new_object();

        obj.set_mature();

        assert!(obj.generation().is_mature());
    }

    #[test]
    fn test_object_pointers_without_pointers() {
        let obj = new_object();

        assert_eq!(obj.pointers().len(), 0);
    }

    #[test]
    fn test_object_pointers_with_pointers() {
        let mut obj = new_object();
        let name = "test".to_string();

        obj.add_method(name.clone(), fake_pointer());
        obj.add_attribute(name.clone(), fake_pointer());
        obj.add_constant(name.clone(), fake_pointer());

        assert_eq!(obj.pointers().len(), 3);
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
}
