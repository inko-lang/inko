//! Struct used for storing values in registers.
//!
//! Registers can be set in any particular order. However, reading from a
//! register that is not set can lead to bogus data being returned.
use crate::chunk::Chunk;
use crate::gc::work_list::WorkList;
use crate::object_pointer::ObjectPointer;

/// Structure used for storing temporary values of a scope.
pub struct Register {
    pub values: Chunk<ObjectPointer>,
}

impl Register {
    /// Creates a new Register.
    pub fn new(amount: usize) -> Register {
        Register {
            values: Chunk::new(amount),
        }
    }

    /// Sets the value of the given register.
    pub fn set(&mut self, register: usize, value: ObjectPointer) {
        self.values[register] = value;
    }

    /// Returns the value of a register.
    pub fn get(&self, register: usize) -> ObjectPointer {
        self.values[register]
    }

    /// Pushes all pointers in this register into the supplied vector.
    pub fn push_pointers(&self, pointers: &mut WorkList) {
        for index in 0..self.values.len() {
            let pointer = &self.values[index];

            if !pointer.is_null() {
                pointers.push(pointer.pointer());
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
        let mut register = Register::new(6);
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        register.set(0, pointer);
        assert!(register.get(0) == pointer);

        register.set(5, pointer);
        assert!(register.get(5) == pointer);
    }

    #[test]
    fn test_push_pointers() {
        let mut register = Register::new(2);

        let pointer1 = ObjectPointer::new(0x1 as RawObjectPointer);
        let pointer2 = ObjectPointer::new(0x2 as RawObjectPointer);

        register.set(0, pointer1);
        register.set(1, pointer2);

        let mut pointers = WorkList::new();

        register.push_pointers(&mut pointers);

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
