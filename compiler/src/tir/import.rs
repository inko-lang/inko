/// A symbol to import.
#[derive(Debug)]
pub struct Symbol {
    /// The name of the symbol to import.
    pub import_name: String,

    /// The name to expose the symbol as.
    pub import_as: String,

    pub line: usize,
    pub column: usize,
}

impl Symbol {
    pub fn new(
        import_name: String,
        import_as: String,
        line: usize,
        column: usize,
    ) -> Self {
        Symbol {
            import_name: import_name,
            import_as: import_as,
            line: line,
            column: column,
        }
    }
}
