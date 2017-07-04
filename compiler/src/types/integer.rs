use rc_cell::RcCell;
use types::object::Object;

#[derive(Debug)]
pub struct Integer {
    pub prototype: RcCell<Object>,
}

impl Integer {
    pub fn new(prototype: RcCell<Object>) -> RcCell<Integer> {
        RcCell::new(Integer { prototype: prototype })
    }
}
