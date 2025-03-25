//! Formatters for diagnostics.
use crate::diagnostics::{enable_colors, Diagnostic, Diagnostics};
use std::env::current_dir;
use std::path::PathBuf;

/// A type used for presenting diagnostics to the user.
pub(crate) trait Presenter {
    fn present(&self, diagnostics: &Diagnostics);
}

/// Print diagnostics in a compact text form, optionally enabling the use of
/// colors.
///
/// The resulting output looks like this:
///
///     path/to/file.inko:line:column warning(example): this is a warning
pub(crate) struct TextPresenter {
    working_directory: PathBuf,
    colors: bool,
}

impl TextPresenter {
    pub(crate) fn new(colors: bool) -> Self {
        let working_directory =
            current_dir().unwrap_or_else(|_| PathBuf::new());

        Self { working_directory, colors }
    }

    pub(crate) fn without_colors() -> Self {
        Self::new(false)
    }

    pub(crate) fn with_colors() -> Self {
        Self::new(enable_colors())
    }

    fn present_diagnostic(&self, diagnostic: &Diagnostic) {
        let loc = &diagnostic.location();
        let abs_path = diagnostic.file().as_path();
        let rel_path = abs_path
            .strip_prefix(&self.working_directory)
            .unwrap_or(abs_path)
            .to_string_lossy();

        let kind = if diagnostic.is_error() {
            format!("{}({})", self.red(self.bold("error")), diagnostic.id())
        } else {
            format!(
                "{}({})",
                self.yellow(self.bold("warning")),
                diagnostic.id()
            )
        };

        eprintln!(
            "{}:{}:{} {}: {}",
            rel_path,
            loc.line_start,
            loc.column_start,
            kind,
            diagnostic.message()
        );
    }

    fn red<S: Into<String>>(&self, text: S) -> String {
        self.color(31, text)
    }

    fn yellow<S: Into<String>>(&self, text: S) -> String {
        self.color(33, text)
    }

    fn bold<S: Into<String>>(&self, text: S) -> String {
        self.color(1, text)
    }

    fn color<S: Into<String>>(&self, code: usize, text: S) -> String {
        if !self.colors {
            return text.into();
        };

        format!("\x1b[{}m{}\x1b[0m", code, text.into())
    }
}

impl Presenter for TextPresenter {
    fn present(&self, diagnostics: &Diagnostics) {
        for diag in diagnostics.iter() {
            self.present_diagnostic(diag);
        }
    }
}

/// A type that presents diagnostics as JSON.
pub(crate) struct JsonPresenter {}

impl JsonPresenter {
    pub(crate) fn new() -> Self {
        Self {}
    }

    fn to_json(&self, diagnostic: &Diagnostic) -> String {
        let loc = diagnostic.location();

        format!(
            "{{\"id\": {:?}, \"level\": {:?}, \"file\": {:?}, \"lines\": [{}, {}], \"columns\": [{}, {}], \"message\": {:?}}}",
            diagnostic.id().to_string(),
            diagnostic.kind().to_string(),
            diagnostic.file().to_string_lossy(),
            loc.line_start,
            loc.line_end,
            loc.column_start,
            loc.column_end,
            diagnostic.message()
        )
    }
}

impl Presenter for JsonPresenter {
    fn present(&self, diagnostics: &Diagnostics) {
        let mut entries = Vec::new();

        for diag in diagnostics.iter() {
            entries.push(self.to_json(diag));
        }

        eprintln!("[{}]", entries.join(","));
    }
}
