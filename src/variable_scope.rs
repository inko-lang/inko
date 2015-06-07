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
pub struct VariableScope {
    /// The local variables in the current scope.
    pub local_variables: Vec<RcObject>,

    /// The parent variable scope, if any.
    pub parent: Option<Box<VariableScope>>
}

impl VariableScope {
    /// Creates a new, empty VariableScope.
    pub fn new() -> VariableScope {
        VariableScope {
            local_variables: Vec::new(),
            parent: None
        }
    }

    /// Boxes and sets the current scope's parent.
    pub fn set_parent(&mut self, parent: VariableScope) {
        self.parent = Some(Box::new(parent));
    }

    /// Pushes a new variable into the current scope.
    pub fn add(&mut self, value: RcObject) {
        self.local_variables.push(value);
    }

    /// Inserts a variable at a specific place.
    pub fn insert(&mut self, index: usize, value: RcObject) {
        self.local_variables.insert(index, value);
    }

    /// Returns a local variable wrapped in an Option.
    pub fn get(&self, index: usize) -> Option<RcObject> {
        match self.local_variables.get(index) {
            Some(object) => { Some(object.clone()) },
            None         => { None }
        }
    }
}
