use mutability::Mutability;
use std::collections::HashMap;
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

    pub fn define<T: ToString>(
        &mut self,
        name: T,
        kind: Type,
        mutability: Mutability,
    ) -> RcSymbol {
        let sym = RcSymbol::new(Symbol {
            name: name.to_string(),
            kind: kind,
            index: self.map.len(),
            mutability: mutability,
        });

        self.map.insert(name.to_string(), sym.clone());

        sym
    }

    pub fn lookup(&self, name: &str) -> Option<RcSymbol> {
        self.map.get(name).cloned()
    }
}
