//! Compiler state accessible to compiler passes.
use crate::config::Config;
use crate::diagnostics::Diagnostics;
use types::Database;

/// State that is accessible by the compiler passes.
///
/// This is stored in a separate type/module so we don't end up with a circular
/// dependency between a compiler and its passes.
pub(crate) struct State {
    pub(crate) config: Config,
    pub(crate) diagnostics: Diagnostics,
    pub(crate) db: Database,
}

impl State {
    pub(crate) fn new(config: Config) -> Self {
        let diagnostics = Diagnostics::new();
        let db = Database::new();

        Self { config, diagnostics, db }
    }
}
