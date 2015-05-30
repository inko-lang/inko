use object::RcObject;

/// Structure for storing local variables
///
/// A VariableScope contains all the local variables of a given scope. These are
/// stored and accessed by index, it's up to the compiler to provide/use the
/// correct indexes.
///
/// In the case of closures the local variables can simply be copied over into a
/// new VariableScope (instead of setting some sort of parent scope). Due to
/// threads having their own memory (and variable scope) there's no need for
/// synchronization either.
///
pub struct VariableScope<'l> {
    /// The local variables in the current scope.
    pub local_variables: Vec<RcObject<'l>>
}

impl<'l> VariableScope<'l> {
    /// Creates a new, empty VariableScope.
    pub fn new() -> VariableScope<'l> {
        VariableScope {
            local_variables: Vec::new()
        }
    }

    /// Adds a new variable to the current scope.
    pub fn add(&mut self, variable: RcObject<'l>) {
        self.local_variables.push(variable);
    }

    /// Returns a local variable wrapped in an Option.
    pub fn get(&self, index: usize) -> Option<RcObject<'l>> {
        match self.local_variables.get(index) {
            Option::Some(object) => { Option::Some(object.clone()) },
            Option::None         => { Option::None }
        }
    }
}
