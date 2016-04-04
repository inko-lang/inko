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

use object::RcObject;

/// Structure used for storing temporary values of a scope.
pub struct Register {
    values: HashMap<usize, RcObject>
}

impl Register {
    /// Creates a new Register.
    pub fn new() -> Register {
        Register { values: HashMap::new() }
    }

    /// Sets the value of the given register.
    ///
    /// # Examples
    ///
    ///     let mut register = Register::new();
    ///     let obj          = Object::new(ObjectValue::Integer(10));
    ///
    ///     register.set(0, obj);
    ///
    pub fn set(&mut self, register: usize, value: RcObject) {
        self.values.insert(register, value);
    }

    /// Returns the value of a register.
    ///
    /// Register values are optional to allow for instructions such as
    /// "goto_if_undef", as such this method returns an Option.
    ///
    /// # Examples
    ///
    ///     let mut register = Register::new();
    ///     let obj          = Object::new(ObjectValue::Integer(10));
    ///
    ///     register.set(0, obj);
    ///
    ///     register.get(0) // => Option<...>
    ///
    pub fn get(&self, register: usize) -> Option<RcObject> {
        match self.values.get(&register) {
            Some(object) => { Some(object.clone()) },
            None         => { None }
        }
    }
}
