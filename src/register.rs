use std::collections::HashMap;

use object::RcObject;

/// Structure used for storing temporary values of a scope.
///
/// The Register is used for storing temporary values in a slot. For example,
/// take the following code:
///
///     number = 10 + 20
///
/// Here both 10 and 20 are temporary values that would be stored in a register
/// slot. The result of this expression would also be stored in a slot before
/// being assigned to the "number" variable.
///
pub struct Register<'l> {
    slots: HashMap<usize, RcObject<'l>>
}

impl<'l> Register<'l> {
    /// Creates a new Register.
    pub fn new() -> Register<'l> {
        Register { slots: HashMap::new() }
    }

    /// Sets the value of the given slot.
    ///
    /// # Examples
    ///
    ///     let mut register = Register::new();
    ///     let obj          = Object::with_rc(ObjectValue::Integer(10));
    ///
    ///     register.set(0, obj);
    ///
    pub fn set(&mut self, slot: usize, value: RcObject<'l>) {
        self.slots.insert(slot, value);
    }

    /// Returns the value of a slot.
    ///
    /// Slot values are optional to allow for instructions such as
    /// "goto_if_undef", as such this method returns an Option.
    ///
    /// # Examples
    ///
    ///     let mut register = Register::new();
    ///     let obj          = Object::with_rc(ObjectValue::Integer(10));
    ///
    ///     register.set(0, obj);
    ///
    ///     register.get(0) // => Option<...>
    ///
    pub fn get(&self, slot: usize) -> Option<RcObject<'l>> {
        match self.slots.get(&slot) {
            Some(object) => { Some(object.clone()) },
            None         => { None }
        }
    }
}
