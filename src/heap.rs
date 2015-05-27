use object::{Object, ObjectValue};

const DEFAULT_CAPACITY: usize = 1024;

pub struct Heap<'l> {
    members: Vec<Object<'l>>
}

impl <'l> Heap<'l> {
    pub fn new() -> Heap<'l> {
        Heap::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Heap<'l> {
        Heap { members: Vec::with_capacity(capacity) }
    }

    pub fn capacity(&self) -> usize {
        self.members.capacity()
    }

    pub fn allocate(&mut self, value: ObjectValue<'l>) -> &Object<'l> {
        let object = Object::new(value);

        self.members.push(object);

        self.members.last().unwrap()
    }

    pub fn allocate_integer(&mut self, value: isize) -> &Object<'l> {
        self.allocate(ObjectValue::Integer(value))
    }
}
