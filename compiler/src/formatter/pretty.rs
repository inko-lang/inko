use std::fmt::Write;

use ansi_term::Colour::{Red, Yellow};
use ansi_term::Style;

use compiler::diagnostic::{Diagnostic, DiagnosticLevel};
use compiler::diagnostics::Diagnostics;
use formatter::Formatter;

pub struct Pretty;

impl Pretty {
    pub fn new() -> Self {
        Pretty
    }

    fn format_error(&self, message: &Diagnostic, output: &mut String) {
        let level = Red.bold().paint("ERROR:");
        let text = Style::new().bold().paint(message.message.clone());

        write!(output,
               "{} {}\n File: {} line {}, column {}\n",
               level,
               text,
               message.path,
               message.line,
               message.column)
            .unwrap();
    }

    fn format_warning(&self, message: &Diagnostic, output: &mut String) {
        let level = Yellow.bold().paint("WARNING:");
        let text = Style::new().bold().paint(message.message.clone());

        write!(output,
               "{} {}\n   File: {} line {}, column {}\n",
               level,
               text,
               message.path,
               message.line,
               message.column)
            .unwrap();
    }
}

impl Formatter for Pretty {
    fn format(&self, diagnostics: &Diagnostics) -> String {
        let mut output = String::new();

        for diag in diagnostics.iter() {
            match diag.level {
                DiagnosticLevel::Warning => {
                    self.format_warning(diag, &mut output);
                }
                DiagnosticLevel::Error => {
                    self.format_error(diag, &mut output);
                }
            }
        }

        output
    }
}
