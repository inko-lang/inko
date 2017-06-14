use tir::types::Type;

#[derive(Debug)]
pub struct RegisterValue {
    pub index: usize,
    pub value_type: Type,
}

#[derive(Copy, Clone, Debug)]
pub struct Register {
    pub index: usize,
}

#[derive(Debug)]
pub struct Registers {
    entries: Vec<RegisterValue>,
}

impl RegisterValue {
    pub fn new(index: usize, value_type: Type) -> Self {
        RegisterValue { index: index, value_type: value_type }
    }
}

impl Register {
    pub fn new(index: usize) -> Self {
        Register { index: index }
    }
}

impl Registers {
    pub fn new() -> Self {
        Registers { entries: Vec::new() }
    }

    pub fn reserve(&mut self) -> Register {
        let index = self.entries.len();
        let register = RegisterValue::new(index, Type::Unknown);

        self.entries.push(register);

        Register::new(index)
    }

    pub fn get(&self, register: &Register) -> Option<&RegisterValue> {
        self.entries.get(register.index)
    }
}
