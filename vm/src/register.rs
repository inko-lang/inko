//! Struct used for storing values in registers.
//!
//! Registers can be set in any particular order. However, reading from a
//! register that is not set can lead to bogus data being returned.

use chunk::Chunk;
use object_pointer::{ObjectPointer, ObjectPointerPointer};

/// Structure used for storing temporary values of a scope.
pub struct Register {
    pub values: Chunk<ObjectPointer>,
}

pub struct PointerIterator<'a> {
    register: &'a Register,
    index: usize,
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
    pub fn push_pointers(&self, pointers: &mut Vec<ObjectPointerPointer>) {
        for pointer in self.pointers() {
            pointers.push(pointer);
        }
    }

    /// Returns an iterator for traversing all pointers in this register.
    pub fn pointers(&self) -> PointerIterator {
        PointerIterator {
            register: self,
            index: 0,
        }
    }
}

impl<'a> Iterator for PointerIterator<'a> {
    type Item = ObjectPointerPointer;

    fn next(&mut self) -> Option<ObjectPointerPointer> {
        while self.index < self.register.values.len() {
            let ref local = self.register.values[self.index];

            self.index += 1;

            if !local.is_null() {
                return Some(local.pointer());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{ObjectPointer, RawObjectPointer};

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

        let mut pointers = Vec::new();

        register.push_pointers(&mut pointers);

        assert_eq!(pointers.len(), 2);

        // The returned pointers should allow updating of what's stored in the
        // register without copying anything.
        for pointer_pointer in pointers {
            let pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(register.get(0).raw.raw as usize, 0x4);
        assert_eq!(register.get(1).raw.raw as usize, 0x4);
    }

    #[test]
    fn test_pointers() {
        let mut register = Register::new(2);

        let pointer1 = ObjectPointer::new(0x1 as RawObjectPointer);
        let pointer2 = ObjectPointer::new(0x2 as RawObjectPointer);

        register.set(0, pointer1);
        register.set(1, pointer2);

        let mut iterator = register.pointers();

        assert!(iterator.next().unwrap().get() == &pointer1);
        assert!(iterator.next().unwrap().get() == &pointer2);
        assert!(iterator.next().is_none());
    }
}
