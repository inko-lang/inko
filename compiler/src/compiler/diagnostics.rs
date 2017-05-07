//! Collections of diagnostic messages

use std::slice;

use compiler::diagnostic::{Diagnostic, DiagnosticLevel};

pub struct Diagnostics {
    entries: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics { entries: Vec::new() }
    }

    pub fn error(&mut self,
                 path: &str,
                 message: String,
                 line: usize,
                 column: usize) {
        self.entries
            .push(Diagnostic::error(path.to_string(), message, line, column));
    }

    pub fn warn(&mut self,
                path: &str,
                message: String,
                line: usize,
                column: usize) {
        self.entries
            .push(Diagnostic::warning(path.to_string(), message, line, column));
    }

    pub fn append(&mut self, mut other: Diagnostics) {
        self.entries.append(&mut other.entries);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn has_errors(&self) -> bool {
        self.entries.iter().any(|ref entry| match entry.level {
            DiagnosticLevel::Error => true,
            DiagnosticLevel::Warning => false,
        })
    }

    pub fn iter(&self) -> slice::Iter<Diagnostic> {
        self.entries.iter()
    }
}
