//! Object Metadata
//!
//! The ObjectHeader struct stores metadata associated with an Object, such as
//! the name, attributes, constants and methods. An ObjectHeader struct is only
//! allocated when actually needed.

use std::collections::HashMap;

use compiled_code::RcCompiledCode;
use object::RcObject;

pub struct ObjectHeader {
    /// The name of the object, used in error messages if present.
    pub name: Option<String>,

    pub attributes: HashMap<String, RcObject>,
    pub constants: HashMap<String, RcObject>,
    pub methods: HashMap<String, RcCompiledCode>,

    /// When set to "true" this object won't be GC'd.
    pub pinned: bool
}

impl ObjectHeader {
    pub fn new() -> ObjectHeader {
        ObjectHeader {
            name: None,
            attributes: HashMap::new(),
            constants: HashMap::new(),
            methods: HashMap::new(),
            pinned: false
        }
    }
}
