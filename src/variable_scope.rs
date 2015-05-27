use std::collections::HashMap;
use object::Object;

pub struct VariableScope<'l> {
    pub local_variables: HashMap<&'l str, &'l Object<'l>>,
    pub instance_variables: HashMap<&'l str, &'l Object<'l>>,
    pub parent: Option<&'l VariableScope<'l>>
}

impl<'l> VariableScope<'l> {
    pub fn new() -> VariableScope<'l> {
        VariableScope {
            local_variables: HashMap::new(),
            instance_variables: HashMap::new(),
            parent: Option::None
        }
    }

    pub fn set_parent(&mut self, parent: &'l VariableScope<'l>) {
        self.parent = Option::Some(parent);
    }
}
