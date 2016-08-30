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
    pub fn pointers(&self) -> Vec<ObjectPointer> {
        let mut pointers = Vec::new();

        if let Some(prototype) = self.prototype() {
            pointers.push(prototype);
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
