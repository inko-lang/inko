use rc_cell::RcCell;
use types::object::Object;

/// The type "database" containing the top level object and various built-in
/// prototype objects.
pub struct Database {
    /// The top-level object in which all other objects are defined.
    pub top_level: RcCell<Object>,

    /// The prototype to use for blocks.
    pub block_prototype: RcCell<Object>,
}

impl Database {
    pub fn new() -> Self {
        Database {
            top_level: RcCell::new(Object::with_name("<top-level>")),
            block_prototype: RcCell::new(Object::with_name("<block prototype>")),
        }
    }
}
