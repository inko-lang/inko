//! Structure for tracking the state of the compilation process of Inko modules.
use std::collections::HashMap;
use std::rc::Rc;

use config::Config;
use diagnostics::Diagnostics;
use tir::module::Module;
use types::database::Database as TypeDatabase;

pub struct State {
    pub config: Rc<Config>,

    /// Any diagnostics that were produced when compiling modules.
    pub diagnostics: Diagnostics,

    /// All the compiled modules, mapped to their names. The values of this hash
    /// are explicitly set to None when:
    ///
    /// * The module was found and is about to be processed for the first time
    /// * The module could not be found
    ///
    /// This prevents recursive imports from causing the compiler to get stuck
    /// in a loop.
    pub modules: HashMap<String, Option<Module>>,

    /// The database storing all type information.
    pub typedb: TypeDatabase,
}

impl State {
    pub fn new(config: Rc<Config>) -> Self {
        State {
            config: config,
            diagnostics: Diagnostics::new(),
            modules: HashMap::new(),
            typedb: TypeDatabase::new(),
        }
    }
}
