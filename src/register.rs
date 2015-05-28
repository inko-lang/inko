use std::collections::HashMap;

use object::RcObject;

pub struct Register<'l> {
    slots: HashMap<isize, RcObject<'l>>
}

impl<'l> Register<'l> {
    pub fn new() -> Register<'l> {
        Register { slots: HashMap::new() }
    }

    pub fn set(&mut self, slot: isize, value: RcObject<'l>) {
        self.slots.insert(slot, value);
    }

    pub fn get(&self, slot: isize) -> RcObject<'l> {
        self.slots.get(&slot).unwrap().clone()
    }
}
