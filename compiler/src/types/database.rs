use rc_cell::RcCell;
use types::object::Object;

/// The type "database" containing the top level object and various built-in
/// prototype objects.
pub struct Database {
    /// The top-level object in which all other objects are defined.
    pub top_level: RcCell<Object>,
    pub block_prototype: RcCell<Object>,
    pub integer_prototype: RcCell<Object>,
    pub float_prototype: RcCell<Object>,
    pub string_prototype: RcCell<Object>,
    pub array_prototype: RcCell<Object>,
    pub boolean_prototype: RcCell<Object>,
}

impl Database {
    pub fn new() -> Self {
        Database {
            top_level: Object::with_name("<top-level>"),
            block_prototype: Object::with_name("<block prototype>"),
            integer_prototype: Object::with_name("<integer prototype>"),
            float_prototype: Object::with_name("<float prototype>"),
            string_prototype: Object::with_name("<string prototype>"),
            array_prototype: Object::with_name("<array prototype>"),
            boolean_prototype: Object::with_name("<boolean prototype>"),
        }
    }
}
