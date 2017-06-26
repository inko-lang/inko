use std::collections::HashMap;
use mutability::Mutability;
use symbol::{Symbol, RcSymbol};
use types::Type;

/// A wrapper around a HashMap for storing symbols by their names.
#[derive(Debug)]
pub struct SymbolTable {
    map: HashMap<String, RcSymbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable { map: HashMap::new() }
    }

    pub fn define(
        &mut self,
        name: String,
        vtype: Type,
        mutability: Mutability,
    ) -> RcSymbol {
        let sym = RcSymbol::new(Symbol {
            name: name.clone(),
            value_type: vtype,
            index: self.map.len(),
            mutability: mutability,
        });

        self.map.insert(name, sym.clone());

        sym
    }

    pub fn lookup(&self, name: &String) -> Option<RcSymbol> {
        self.map.get(name).cloned()
    }
}
