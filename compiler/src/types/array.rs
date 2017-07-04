use rc_cell::RcCell;
use types::object::Object;

#[derive(Debug)]
pub struct Array {
    pub prototype: RcCell<Object>,
}

impl Array {
    pub fn new(prototype: RcCell<Object>) -> RcCell<Array> {
        RcCell::new(Array { prototype: prototype })
    }
}
