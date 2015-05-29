use object::RcObject;

pub struct VariableScope<'l> {
    pub local_variables: Vec<RcObject<'l>>,
    pub parent: Option<&'l VariableScope<'l>>
}

impl<'l> VariableScope<'l> {
    pub fn new() -> VariableScope<'l> {
        VariableScope {
            local_variables: Vec::new(),
            parent: Option::None
        }
    }

    pub fn set_parent(&mut self, parent: &'l VariableScope<'l>) {
        self.parent = Option::Some(parent);
    }

    pub fn add(&mut self, variable: RcObject<'l>) {
        self.local_variables.push(variable);
    }

    pub fn get(&self, index: usize) -> Option<RcObject<'l>> {
        match self.local_variables.get(index) {
            Option::Some(object) => { Option::Some(object.clone()) },
            Option::None         => { Option::None }
        }
    }
}
