use rc_cell::RcCell;
use types::object::Object;

#[derive(Debug)]
pub struct Float {
    pub prototype: RcCell<Object>,
}

impl Float {
    pub fn new(prototype: RcCell<Object>) -> RcCell<Float> {
        RcCell::new(Float { prototype: prototype })
    }
}
