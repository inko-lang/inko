//! Struct used for storing values in registers.
//!
//! Registers can be set in any particular order. However, reading from a
//! register that is not set can lead to bogus data being returned.
use crate::chunk::Chunk;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};

/// Structure used for storing temporary values of a scope.
pub struct Registers {
    pub values: Chunk<ObjectPointer>,
}

impl Registers {
    /// Creates a new Registers.
    pub fn new(amount: u16) -> Registers {
        Registers {
            values: Chunk::new(amount as usize),
        }
    }

    /// Sets the value of the given register.
    pub fn set(&mut self, register: u16, value: ObjectPointer) {
        self.values[register as usize] = value;
    }

    /// Returns the value of a register.
    pub fn get(&self, register: u16) -> ObjectPointer {
        self.values[register as usize]
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        for index in 0..self.values.len() {
            let pointer = &self.values[index];

            if !pointer.is_null() {
                callback(pointer.pointer());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_pointer::{ObjectPointer, RawObjectPointer};

    #[test]
    fn test_set_get() {
        let mut register = Registers::new(6);
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        register.set(0, pointer);
        assert!(register.get(0) == pointer);

        register.set(5, pointer);
        assert!(register.get(5) == pointer);
    }

    #[test]
    fn test_each_pointer() {
        let mut register = Registers::new(2);

        let pointer1 = ObjectPointer::new(0x1 as RawObjectPointer);
        let pointer2 = ObjectPointer::new(0x2 as RawObjectPointer);

        register.set(0, pointer1);
        register.set(1, pointer2);

        let mut pointers = Vec::new();

        register.each_pointer(|ptr| pointers.push(ptr));

        // The returned pointers should allow updating of what's stored in the
        // register without copying anything.
        while let Some(pointer_pointer) = pointers.pop() {
            let pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(register.get(0).raw.raw as usize, 0x4);
        assert_eq!(register.get(1).raw.raw as usize, 0x4);
    }
}
