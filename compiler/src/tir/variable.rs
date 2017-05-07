use std::collections::HashMap;
use tir::types::Type;

#[derive(Clone, Debug)]
pub enum Mutability {
    Immutable,
    Mutable,
}

#[derive(Debug)]
pub struct VariableInfo {
    pub name: String,
    pub value_type: Type,
    pub mutability: Mutability,
}

#[derive(Copy, Clone, Debug)]
pub struct Variable {
    pub index: usize,
}

#[derive(Debug)]
pub struct Scope {
    variables: Vec<VariableInfo>,
    mapping: HashMap<String, Variable>,
}

impl VariableInfo {
    pub fn new(name: String, value_type: Type, mutability: Mutability) -> Self {
        VariableInfo {
            name: name,
            value_type: value_type,
            mutability: mutability,
        }
    }
}

impl Variable {
    pub fn new(index: usize) -> Self {
        Variable { index: index }
    }
}

impl Scope {
    pub fn new() -> Self {
        Scope {
            variables: Vec::new(),
            mapping: HashMap::new(),
        }
    }

    pub fn lookup(&self, name: &String) -> Option<Variable> {
        self.mapping.get(name).cloned()
    }

    pub fn define(&mut self, name: String, mutability: Mutability) -> Variable {
        let info = VariableInfo::new(name.clone(), Type::Unknown, mutability);
        let index = self.variables.len();
        let variable = Variable::new(index);

        self.variables.push(info);
        self.mapping.insert(name, variable);

        variable
    }
}
