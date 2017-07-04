use rc_cell::RcCell;
use types::object::Object;

#[derive(Debug)]
pub struct String {
    pub prototype: RcCell<Object>,
}

impl String {
    pub fn new(prototype: RcCell<Object>) -> RcCell<String> {
        RcCell::new(String { prototype: prototype })
    }
}
