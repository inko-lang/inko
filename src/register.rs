use std::collections::HashMap;

pub struct Register {
    slots: HashMap<usize, usize>
}

impl Register {
    pub fn new() -> Register {
        Register { slots: HashMap::new() }
    }

    pub fn set(&mut self, slot: usize, value: usize) {
        self.slots.insert(slot, value);
    }

    pub fn get(&self, slot: usize) -> &usize {
        self.slots.get(&slot).unwrap()
    }
}
