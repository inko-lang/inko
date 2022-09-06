///! Virtual machine registers
use crate::chunk::Chunk;
use crate::mem::Pointer;

/// A collection of virtual machine registers.
pub(crate) struct Registers {
    pub values: Chunk<Pointer>,
}

impl Registers {
    /// Creates a new Registers.
    pub(crate) fn new(amount: u16) -> Registers {
        Registers { values: Chunk::new(amount as usize) }
    }

    /// Sets the value of the given register.
    pub(crate) fn set(&mut self, register: u16, value: Pointer) {
        unsafe { self.values.set(register as usize, value) };
    }

    /// Returns the value of a register.
    pub(crate) fn get(&self, register: u16) -> Pointer {
        unsafe { *self.values.get(register as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Pointer;

    #[test]
    fn test_set_get() {
        let mut register = Registers::new(6);
        let pointer = Pointer::new(0x4 as *mut u8);

        register.set(0, pointer);
        assert!(register.get(0) == pointer);

        register.set(5, pointer);
        assert!(register.get(5) == pointer);
    }
}
