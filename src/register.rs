use std::collections::HashMap;

use object::RcObject;

pub struct Register<'l> {
    slots: HashMap<usize, RcObject<'l>>
}

impl<'l> Register<'l> {
    pub fn new() -> Register<'l> {
        Register { slots: HashMap::new() }
    }

    pub fn set(&mut self, slot: usize, value: RcObject<'l>) {
        self.slots.insert(slot, value);
    }

    pub fn get(&self, slot: usize) -> Option<RcObject<'l>> {
        match self.slots.get(&slot) {
            Option::Some(object) => { Option::Some(object.clone()) },
            Option::None         => { Option::None }
        }
    }
}
