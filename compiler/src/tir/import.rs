use symbol::SymbolPointer;

/// The type of symbol to import.
#[derive(Debug)]
pub enum SymbolKind {
    Module,
    Constant,
}

/// A symbol to import.
#[derive(Debug)]
pub struct Symbol {
    kind: SymbolKind,
    import_name: String,
    global: SymbolPointer,
    line: usize,
    column: usize,
}

impl Symbol {
    pub fn module(
        import_name: String,
        global: SymbolPointer,
        line: usize,
        column: usize,
    ) -> Self {
        Symbol {
            kind: SymbolKind::Module,
            import_name: import_name,
            global: global,
            line: line,
            column: column,
        }
    }

    pub fn constant(
        import_name: String,
        global: SymbolPointer,
        line: usize,
        column: usize,
    ) -> Self {
        Symbol {
            kind: SymbolKind::Constant,
            import_name: import_name,
            global: global,
            line: line,
            column: column,
        }
    }
}
