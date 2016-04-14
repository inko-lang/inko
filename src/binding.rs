use std::sync::{Arc, RwLock};

use object::RcObject;

pub struct Binding {
    pub self_object: RcObject,
    pub variables: Vec<RcObject>
}

pub type RcBinding = Arc<RwLock<Binding>>;

impl Binding {
    pub fn new(self_object: RcObject) -> RcBinding {
        let bind = Binding { self_object: self_object, variables: Vec::new() };

        Arc::new(RwLock::new(bind))
    }
}
