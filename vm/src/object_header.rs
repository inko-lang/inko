//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods. An ObjectHeader struct is only
//! allocated when actually needed.

use std::collections::HashMap;

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
}
