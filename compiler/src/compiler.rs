//! Compiler for generating bytecode and object files.
use std::rc::Rc;

use backend::ivm::Ivm as IvmBackend;
use config::Config;
use diagnostics::Diagnostics;
use state::State;
use tir::builder::Builder;

pub struct Compiler {
    state: State,
}

impl Compiler {
    pub fn new(config: Config) -> Self {
        let state = State::new(Rc::new(config));

        Compiler { state: state }
    }

    pub fn compile(&mut self, path: String) {
        let mod_opt = Builder::new(&mut self.state).build_main(path);

        if let Some(module) = mod_opt {
            IvmBackend::new(&mut self.state).compile(module);
        }
    }

    pub fn has_errors(&self) -> bool {
        self.state.diagnostics.has_errors()
    }

    pub fn has_diagnostics(&self) -> bool {
        self.state.diagnostics.len() > 0
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.state.diagnostics
    }
}
