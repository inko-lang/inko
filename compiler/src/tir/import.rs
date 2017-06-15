//! Inko modules to import
use tir::variable::Variable;

/// The type of symbol to import.
#[derive(Debug, Clone)]
pub enum SymbolKind {
    Module,
    Constant,
}

/// A symbol to import.
#[derive(Debug, Clone)]
pub struct Symbol {
    kind: SymbolKind,
    import_name: String,
    global: Variable,
    line: usize,
    column: usize,
}

impl Symbol {
    pub fn module(
        import_name: String,
        global: Variable,
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
        global: Variable,
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
