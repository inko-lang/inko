//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods. An ObjectHeader struct is only
//! allocated when actually needed.

use std::collections::HashMap;

use heap::Heap;
use object_pointer::ObjectPointer;

pub struct ObjectHeader {
    pub attributes: HashMap<String, ObjectPointer>,
    pub constants: HashMap<String, ObjectPointer>,
    pub methods: HashMap<String, ObjectPointer>,

    /// The object to use for constant lookups when a constant is not available
    /// in the prototype hierarchy.
    pub outer_scope: Option<ObjectPointer>
}

impl ObjectHeader {
    pub fn new() -> ObjectHeader {
        ObjectHeader {
            attributes: HashMap::new(),
            constants: HashMap::new(),
            methods: HashMap::new(),
            outer_scope: None
        }
    }

    pub fn copy_to(&self, heap: &mut Heap) -> ObjectHeader {
        let mut copy = ObjectHeader::new();

        for (key, value) in self.attributes.iter() {
            copy.attributes.insert(key.clone(), heap.copy_object(value.clone()));
        }

        for (key, value) in self.constants.iter() {
            copy.constants.insert(key.clone(), heap.copy_object(value.clone()));
        }

        for (key, value) in self.methods.iter() {
            copy.methods.insert(key.clone(), heap.copy_object(value.clone()));
        }

        if let Some(scope) = self.outer_scope.as_ref() {
            copy.outer_scope = Some(heap.copy_object(scope.clone()));
        }

        copy
    }
}
