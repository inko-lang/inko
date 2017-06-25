use std::collections::HashMap;
use mutability::Mutability;
use symbol::{Symbol, SymbolPointer};
use types::Type;

/// A wrapper around a HashMap for storing symbols by their names.
///
/// A SymbolTable owns the Symbols stored in it and will drop them once the
/// table is dropped.
#[derive(Debug)]
pub struct SymbolTable {
    map: HashMap<String, Box<Symbol>>,
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
    ) -> SymbolPointer {
        let sym = Box::new(Symbol {
            name: name.clone(),
            value_type: vtype,
            index: self.map.len(),
            mutability: mutability,
        });

        let pointer = SymbolPointer::new(&*sym);

        self.map.insert(name, sym);

        pointer
    }

    /// Looks up a symbol, returning a pointer to it if the symbol was found.
    pub fn lookup(&self, name: &String) -> Option<SymbolPointer> {
        self.map.get(name).map(|val| SymbolPointer::new(&**val))
    }
}
