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
}
