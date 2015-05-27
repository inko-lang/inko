use std::collections::HashMap;

use object::Object;

pub struct Register<'l> {
    slots: HashMap<isize, &'l Object<'l>>
}

impl<'l> Register<'l> {
    pub fn new() -> Register<'l> {
        Register { slots: HashMap::new() }
    }

    pub fn set(&mut self, slot: isize, value: &'l Object<'l>) {
        self.slots.insert(slot, value);
    }

    pub fn get(&self, slot: isize) -> &'l Object<'l> {
        self.slots.get(&slot).unwrap()
    }
}
