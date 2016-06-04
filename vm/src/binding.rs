use std::sync::{Arc, RwLock};

use object_pointer::ObjectPointer;

pub struct Binding {
    pub self_object: ObjectPointer,
    pub variables: Vec<ObjectPointer>
}

pub type RcBinding = Arc<RwLock<Binding>>;

impl Binding {
    pub fn new(self_object: ObjectPointer) -> RcBinding {
        let bind = Binding { self_object: self_object, variables: Vec::new() };

        Arc::new(RwLock::new(bind))
    }
}
