//! Struct used for storing values in registers.
//!
//! Registers can be set in any particular order.
use object_pointer::{ObjectPointer, ObjectPointerPointer};

/// Structure used for storing temporary values of a scope.
pub struct Register {
    pub values: Vec<ObjectPointer>,
}

pub struct PointerIterator<'a> {
    register: &'a Register,
    index: usize,
}

/// The extra number of slots that should be made available whenever resizing.
const RESIZE_PADDING: usize = 32;

impl Register {
    /// Creates a new Register.
    pub fn new() -> Register {
        Register { values: Vec::new() }
    }

    /// Sets the value of the given register.
    pub fn set(&mut self, register: usize, value: ObjectPointer) {
        if register >= self.values.len() {
            self.values.resize(register + RESIZE_PADDING, ObjectPointer::null());
        }

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
        loop {
            if let Some(local) = self.register.values.get(self.index) {
                self.index += 1;

                if !local.is_null() {
                    return Some(local.pointer());
                }
            } else {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_pointer::{RawObjectPointer, ObjectPointer};

    #[test]
    fn test_set_get() {
        let mut register = Register::new();
        let pointer = ObjectPointer::new(0x4 as RawObjectPointer);

        register.set(0, pointer);
        assert!(register.get(0) == pointer);

        register.set(5, pointer);
        assert!(register.get(5) == pointer);
    }

    #[test]
    #[should_panic]
    fn test_get_invalid() {
        let register = Register::new();

        register.get(2);
    }

    #[test]
    fn test_push_pointers() {
        let mut register = Register::new();

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
            let mut pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(register.get(0).raw.raw as usize, 0x4);
        assert_eq!(register.get(1).raw.raw as usize, 0x4);
    }

    #[test]
    fn test_pointers() {
        let mut register = Register::new();

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
