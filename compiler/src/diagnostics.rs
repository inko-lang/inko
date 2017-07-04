//! Collections of diagnostic messages

use std::slice;

use diagnostic::{Diagnostic, DiagnosticLevel};

pub struct Diagnostics {
    entries: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics { entries: Vec::new() }
    }

    pub fn error<M>(&mut self, path: &str, message: M, line: usize, col: usize)
    where
        M: ToString + Sized,
    {
        self.entries.push(Diagnostic::error(
            path.to_string(),
            message.to_string(),
            line,
            col,
        ));
    }

    pub fn warn<M>(&mut self, path: &str, message: M, line: usize, col: usize)
    where
        M: ToString + Sized,
    {
        self.entries.push(Diagnostic::warning(
            path.to_string(),
            message.to_string(),
            line,
            col,
        ));
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

    pub fn mutable_constant_error(
        &mut self,
        path: &str,
        line: usize,
        col: usize,
    ) {
        self.error(path, "constants can not be defined as mutable", line, col);
    }

    pub fn module_not_found_error(
        &mut self,
        name: &String,
        path: &str,
        line: usize,
        col: usize,
    ) {
        self.error(
            path,
            format!("the module {:?} could not be found", name),
            line,
            col,
        );
    }

    pub fn reassign_immutable_local_error(
        &mut self,
        name: &String,
        path: &str,
        line: usize,
        col: usize,
    ) {
        self.error(
            path,
            format!("cannot re-assign immutable local variable {:?}", name),
            line,
            col,
        );
    }

    pub fn reassign_undefined_local_error(
        &mut self,
        name: &String,
        path: &str,
        line: usize,
        col: usize,
    ) {
        self.error(
            path,
            format!("cannot re-assign undefined local variable {:?}", name),
            line,
            col,
        );
    }

    pub fn unknown_raw_instruction_error(
        &mut self,
        name: &String,
        path: &str,
        line: usize,
        col: usize,
    ) {
        self.error(
            path,
            format!("the raw instruction {:?} does not exist", name),
            line,
            col,
        );
    }
}
