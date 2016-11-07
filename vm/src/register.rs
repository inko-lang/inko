//! Struct used for storing values in registers.
//!
//! For example, take the following code:
//!
//!     number = 10 + 20
//!
//! Here both 10 and 20 are temporary values that would be stored in a register.
//! The result of this expression would also be stored in a register before
//! being assigned to the "number" variable.

use std::collections::HashMap;

use object_pointer::ObjectPointer;

/// Structure used for storing temporary values of a scope.
pub struct Register {
    values: HashMap<usize, ObjectPointer>,
}

impl Register {
    /// Creates a new Register.
    pub fn new() -> Register {
        Register { values: HashMap::new() }
    }

    /// Sets the value of the given register.
    pub fn set(&mut self, register: usize, value: ObjectPointer) {
        self.values.insert(register, value);
    }

    /// Returns the value of a register.
    ///
    /// Register values are optional to allow for instructions such as
    /// "goto_if_undef", as such this method returns an Option.
    pub fn get(&self, register: usize) -> Option<ObjectPointer> {
        match self.values.get(&register) {
            Some(object) => Some(object.clone()),
            None => None,
        }
    }

    /// Pushes all pointers in this register into the supplied vector.
    pub fn push_pointers(&self, pointers: &mut Vec<*const ObjectPointer>) {
        for value in self.values.values() {
            pointers.push(value.as_raw_pointer());
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
        let pointer = ObjectPointer::null();

        assert!(register.get(0).is_none());

        register.set(0, pointer);

        assert!(register.get(0).is_some());
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
            let mut pointer =
                unsafe { &mut *(pointer_pointer as *mut ObjectPointer) };

            pointer.raw.raw = 0x4 as RawObjectPointer;
        }

        assert_eq!(register.get(0).unwrap().raw.raw as usize, 0x4);
        assert_eq!(register.get(1).unwrap().raw.raw as usize, 0x4);
    }
}
