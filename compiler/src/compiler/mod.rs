//! Compiler for generating bytecode and object files.
use std::rc::Rc;

pub mod diagnostic;
pub mod diagnostics;

use config::Config;
use compiler::diagnostics::Diagnostics;
use tir::builder::Builder;

pub struct Compiler {
    config: Rc<Config>,
    diagnostics: Diagnostics,
}

impl Compiler {
    pub fn new(config: Config) -> Self {
        Compiler {
            config: Rc::new(config),
            diagnostics: Diagnostics::new(),
        }
    }

    pub fn compile(&mut self, path: String) {
        let mut builder = Builder::new(self.config.clone());

        if let Some(module) = builder.build(path) {
            println!("{:#?}", module);
        }

        self.diagnostics.append(builder.diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }

    pub fn has_diagnostics(&self) -> bool {
        self.diagnostics.len() > 0
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }
}
