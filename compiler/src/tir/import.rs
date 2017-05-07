//! Inko modules to import

/// A symbol to import.
#[derive(Debug)]
pub enum Symbol {
    /// A single identifier to import.
    Identifier {
        name: String,
        alias: Option<String>,
        line: usize,
        column: usize,
    },
    /// A single constant to import.
    Constant {
        name: String,
        alias: Option<String>,
        line: usize,
        column: usize,
    },
}

impl Symbol {
    pub fn identifier(name: String,
                      alias: Option<String>,
                      line: usize,
                      column: usize)
                      -> Self {
        Symbol::Identifier {
            name: name,
            alias: alias,
            line: line,
            column: column,
        }
    }

    pub fn constant(name: String,
                    alias: Option<String>,
                    line: usize,
                    column: usize)
                    -> Self {
        Symbol::Constant {
            name: name,
            alias: alias,
            line: line,
            column: column,
        }
    }
}
