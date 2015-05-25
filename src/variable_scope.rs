use std::collections::HashMap;

pub struct VariableScope {
    pub local_variables: HashMap<&'l str, usize>,
    pub instance_variables: HashMap<&'l str, usize>,
    pub parent: Option<Box<VariableScope>>
}

impl VariableScope {
    pub fn new() -> VariableScope {
        VariableScope { parent: Option::None }
    }

    pub fn set_parent(&mut self, parent: VariableScope) {
        self.parent = Option::Some(Box::new(parent));
    }
}
