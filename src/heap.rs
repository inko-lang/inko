use object::{Object, RcObject, ObjectValue};

const DEFAULT_CAPACITY: usize = 1024;

pub struct Heap<'l> {
    members: Vec<RcObject<'l>>
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

    pub fn allocate(&mut self, value: ObjectValue<'l>) -> RcObject<'l> {
        let object = Object::new_rc(value);

        self.members.push(object.clone());

        object
    }

    pub fn allocate_object(&mut self) -> RcObject<'l> {
        self.allocate(ObjectValue::None)
    }

    pub fn allocate_integer(&mut self, value: isize) -> RcObject<'l> {
        self.allocate(ObjectValue::Integer(value))
    }

    pub fn allocate_float(&mut self, value: f64) -> RcObject<'l> {
        self.allocate(ObjectValue::Float(value))
    }
}
