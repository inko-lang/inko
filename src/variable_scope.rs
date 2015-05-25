use std::collections::HashMap;

pub struct VariableScope<'l> {
    pub local_variables: HashMap<&'l str, usize>,
    pub instance_variables: HashMap<&'l str, usize>,
    pub parent: Option<Box<VariableScope<'l>>>
}

impl<'l> VariableScope<'l> {
    pub fn new() -> VariableScope<'l> {
        VariableScope {
            local_variables: HashMap::new(),
            instance_variables: HashMap::new(),
            parent: Option::None
        }
    }

    pub fn set_parent(&mut self, parent: VariableScope<'l>) {
        self.parent = Option::Some(Box::new(parent));
    }
}
