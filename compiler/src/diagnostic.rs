/// Diagnostic messages produced during compilation.

pub enum DiagnosticLevel {
    Warning,
    Error,
}

pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub path: String,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl Diagnostic {
    pub fn error(
        path: String,
        message: String,
        line: usize,
        column: usize,
    ) -> Self {
        Diagnostic {
            level: DiagnosticLevel::Error,
            path: path,
            message: message,
            line: line,
            column: column,
        }
    }

    pub fn warning(
        path: String,
        message: String,
        line: usize,
        column: usize,
    ) -> Self {
        Diagnostic {
            level: DiagnosticLevel::Warning,
            path: path,
            message: message,
            line: line,
            column: column,
        }
    }
}
