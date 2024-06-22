//! Type-checking of import statements.
use std::path::PathBuf;

use crate::diagnostics::DiagnosticId;
use crate::hir;
use crate::state::State;
use ast::source_location::SourceLocation;
use types::module_name::ModuleName;
use types::{Database, ModuleId, Symbol};

/// When a symbol is using this name, the source module should be imported
/// instead of the symbol.
const IMPORT_MODULE_ITSELF_NAME: &str = "self";

/// A compiler pass that defines any imported types.
///
/// This pass only defines imported classes, traits and modules.
///
/// Method imports are handled in a separate pass, as we can't process these
/// until other passes have run first.
pub(crate) struct DefineImportedTypes<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineImportedTypes<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) -> bool {
        for module in modules {
            DefineImportedTypes { state, module: module.module_id }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(mut self, module: &mut hir::Module) {
        for expr in &module.expressions {
            if let hir::TopLevelExpression::Import(ref node) = expr {
                self.import(node);
            }
        }
    }

    fn import(&mut self, node: &hir::Import) {
        let source_name = self.import_source(&node.source);
        let source = self.db().module(&source_name.to_string());

        if node.symbols.is_empty() {
            self.import_module(
                source,
                &source_name,
                source_name.tail().to_string(),
                &node.source.last().unwrap().location,
            );
        } else {
            for symbol in &node.symbols {
                let name = symbol.name.name.clone();
                let import_as = symbol.import_as.name.clone();

                if name == IMPORT_MODULE_ITSELF_NAME {
                    self.import_module(
                        source,
                        &source_name,
                        import_as,
                        &symbol.import_as.location,
                    );
                } else {
                    self.import_symbol(source, symbol);
                }
            }
        }
    }

    fn import_module(
        &mut self,
        source: ModuleId,
        source_name: &ModuleName,
        import_as: String,
        location: &SourceLocation,
    ) {
        let name = if import_as == IMPORT_MODULE_ITSELF_NAME {
            source_name.tail().to_string()
        } else {
            import_as
        };

        if self.module.symbol_exists(self.db(), &name) {
            self.state.diagnostics.duplicate_symbol(
                &name,
                self.file(),
                location.clone(),
            );
        } else {
            self.module.new_symbol(self.db_mut(), name, Symbol::Module(source));
        }
    }

    fn import_symbol(&mut self, source: ModuleId, node: &hir::ImportSymbol) {
        let name = &node.name.name;
        let import_as = &node.import_as.name;

        if let Some(symbol) = source.import_symbol(self.db(), name) {
            if self.module.symbol_exists(self.db(), import_as) {
                self.state.diagnostics.duplicate_symbol(
                    import_as,
                    self.file(),
                    node.import_as.location.clone(),
                );
            } else if !symbol.is_visible_to(self.db(), self.module) {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSymbol,
                    format!(
                        "the symbol '{}' is private and can't be imported",
                        name
                    ),
                    self.file(),
                    node.name.location.clone(),
                );
            } else {
                self.module.new_symbol(
                    self.db_mut(),
                    import_as.clone(),
                    symbol,
                );
            }
        } else {
            self.state.diagnostics.undefined_symbol(
                name,
                self.file(),
                node.name.location.clone(),
            );
        }
    }

    fn file(&self) -> PathBuf {
        self.module.file(self.db())
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }

    fn import_source(&self, path: &[hir::Identifier]) -> ModuleName {
        ModuleName::from(
            path.iter().map(|n| n.name.clone()).collect::<Vec<_>>(),
        )
    }
}

/// A compiler pass that collects all externally imported libraries.
pub(crate) struct CollectExternImports<'a> {
    state: &'a mut State,
}

impl<'a> CollectExternImports<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &[hir::Module],
    ) -> bool {
        for module in modules {
            CollectExternImports { state }.run(module);
        }

        !state.diagnostics.has_errors()
    }

    fn run(self, module: &hir::Module) {
        for expr in &module.expressions {
            if let hir::TopLevelExpression::ExternImport(ref node) = expr {
                self.state.libraries.insert(node.source.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::hir;
    use crate::test::{cols, hir_module};
    use std::path::PathBuf;
    use types::module_name::ModuleName;
    use types::{Location, Method, MethodKind, Module, Visibility};

    #[test]
    fn test_import_module() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: Vec::new(),
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        assert!(DefineImportedTypes::run_all(&mut state, &mut modules));

        let tail = "bar".to_string();
        let foo_mod = modules[0].module_id;

        assert!(foo_mod.symbol_exists(&state.db, &tail));
        assert_eq!(
            foo_mod.symbol(&state.db, &tail),
            Some(Symbol::Module(bar_mod))
        );
    }

    #[test]
    fn test_import_duplicate_module() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![
                hir::TopLevelExpression::Import(Box::new(hir::Import {
                    source: vec![hir::Identifier {
                        name: "bar".to_string(),
                        location: cols(1, 1),
                    }],
                    symbols: Vec::new(),
                    location: cols(1, 1),
                })),
                hir::TopLevelExpression::Import(Box::new(hir::Import {
                    source: vec![hir::Identifier {
                        name: "bar".to_string(),
                        location: cols(3, 3),
                    }],
                    symbols: Vec::new(),
                    location: cols(2, 2),
                })),
            ],
        )];

        Module::alloc(&mut state.db, ModuleName::new("bar"), "bar.inko".into());

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(3, 3));
    }

    #[test]
    fn test_import_self() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: "self".to_string(),
                        location: cols(1, 1),
                    },
                    import_as: hir::Identifier {
                        name: "self".to_string(),
                        location: cols(1, 1),
                    },
                    location: cols(1, 1),
                }],
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        assert!(DefineImportedTypes::run_all(&mut state, &mut modules));

        let tail = "bar".to_string();
        let foo_mod = modules[0].module_id;

        assert!(foo_mod.symbol_exists(&state.db, &tail));
        assert_eq!(
            foo_mod.symbol(&state.db, &tail),
            Some(Symbol::Module(bar_mod))
        );
    }

    #[test]
    fn test_import_self_with_alias() {
        let symbol = "bla".to_string();
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: "self".to_string(),
                        location: cols(1, 1),
                    },
                    import_as: hir::Identifier {
                        name: symbol.clone(),
                        location: cols(1, 1),
                    },
                    location: cols(1, 1),
                }],
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        assert!(DefineImportedTypes::run_all(&mut state, &mut modules));

        let foo_mod = modules[0].module_id;

        assert!(foo_mod.symbol_exists(&state.db, &symbol));
        assert!(!foo_mod.symbol_exists(&state.db, "bar"));
        assert_eq!(
            foo_mod.symbol(&state.db, &symbol),
            Some(Symbol::Module(bar_mod))
        );
    }

    #[test]
    fn test_import_duplicate_self() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "self".to_string(),
                            location: cols(1, 1),
                        },
                        import_as: hir::Identifier {
                            name: "bla".to_string(),
                            location: cols(1, 1),
                        },
                        location: cols(1, 1),
                    },
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "self".to_string(),
                            location: cols(2, 2),
                        },
                        import_as: hir::Identifier {
                            name: "bla".to_string(),
                            location: cols(3, 3),
                        },
                        location: cols(1, 1),
                    },
                ],
                location: cols(1, 1),
            }))],
        )];

        Module::alloc(&mut state.db, ModuleName::new("bar"), "bar.inko".into());

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(3, 3));
    }

    #[test]
    fn test_import_symbol() {
        let symbol = "Foo".to_string();
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: "Foo".to_string(),
                        location: cols(1, 1),
                    },
                    import_as: hir::Identifier {
                        name: symbol.clone(),
                        location: cols(1, 1),
                    },
                    location: cols(1, 1),
                }],
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        bar_mod.new_symbol(
            &mut state.db,
            "Foo".to_string(),
            Symbol::Module(bar_mod),
        );

        assert!(DefineImportedTypes::run_all(&mut state, &mut modules));

        let foo_mod = modules[0].module_id;

        assert!(foo_mod.symbol_exists(&state.db, &symbol));
        assert_eq!(
            foo_mod.symbol(&state.db, &symbol),
            Some(Symbol::Module(bar_mod))
        );
    }

    #[test]
    fn test_import_symbol_with_alias() {
        let symbol = "Bar".to_string();
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: "Foo".to_string(),
                        location: cols(1, 1),
                    },
                    import_as: hir::Identifier {
                        name: symbol.clone(),
                        location: cols(1, 1),
                    },
                    location: cols(1, 1),
                }],
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        bar_mod.new_symbol(
            &mut state.db,
            "Foo".to_string(),
            Symbol::Module(bar_mod),
        );

        assert!(DefineImportedTypes::run_all(&mut state, &mut modules));

        let foo_mod = modules[0].module_id;

        assert!(foo_mod.symbol_exists(&state.db, &symbol));
        assert!(!foo_mod.symbol_exists(&state.db, "Foo"));
        assert_eq!(
            foo_mod.symbol(&state.db, &symbol),
            Some(Symbol::Module(bar_mod))
        );
    }

    #[test]
    fn test_import_duplicate_symbol() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![
                    hir::Identifier {
                        name: "foo".to_string(),
                        location: cols(1, 1),
                    },
                    hir::Identifier {
                        name: "bar".to_string(),
                        location: cols(1, 1),
                    },
                ],
                symbols: vec![
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(1, 1),
                        },
                        import_as: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(1, 1),
                        },
                        location: cols(1, 1),
                    },
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(2, 2),
                        },
                        import_as: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(3, 3),
                        },
                        location: cols(2, 2),
                    },
                ],
                location: cols(1, 2),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("foo.bar"),
            "bar.inko".into(),
        );

        bar_mod.new_symbol(
            &mut state.db,
            "Foo".to_string(),
            Symbol::Module(bar_mod),
        );

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(3, 3));
    }

    #[test]
    fn test_import_duplicate_symbol_with_alias() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(1, 1),
                        },
                        import_as: hir::Identifier {
                            name: "Bar".to_string(),
                            location: cols(1, 1),
                        },
                        location: cols(1, 1),
                    },
                    hir::ImportSymbol {
                        name: hir::Identifier {
                            name: "Foo".to_string(),
                            location: cols(2, 2),
                        },
                        import_as: hir::Identifier {
                            name: "Bar".to_string(),
                            location: cols(3, 3),
                        },
                        location: cols(2, 2),
                    },
                ],
                location: cols(1, 2),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        bar_mod.new_symbol(
            &mut state.db,
            "Foo".to_string(),
            Symbol::Module(bar_mod),
        );

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::DuplicateSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(3, 3));
    }

    #[test]
    fn test_import_undefined_symbol() {
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: "Foo".to_string(),
                        location: cols(4, 4),
                    },
                    import_as: hir::Identifier {
                        name: "Foo".to_string(),
                        location: cols(3, 3),
                    },
                    location: cols(2, 2),
                }],
                location: cols(1, 2),
            }))],
        )];

        Module::alloc(&mut state.db, ModuleName::new("bar"), "bar.inko".into());

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(4, 4));
    }

    #[test]
    fn test_import_private_symbol() {
        let symbol = "_foo".to_string();
        let mut state = State::new(Config::new());
        let mut modules = vec![hir_module(
            &mut state,
            ModuleName::new("foo"),
            vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                source: vec![hir::Identifier {
                    name: "bar".to_string(),
                    location: cols(1, 1),
                }],
                symbols: vec![hir::ImportSymbol {
                    name: hir::Identifier {
                        name: symbol.clone(),
                        location: cols(3, 3),
                    },
                    import_as: hir::Identifier {
                        name: symbol.clone(),
                        location: cols(1, 1),
                    },
                    location: cols(1, 1),
                }],
                location: cols(1, 1),
            }))],
        )];

        let bar_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("bar"),
            "bar.inko".into(),
        );

        let foo = Method::alloc(
            &mut state.db,
            bar_mod,
            Location::new(1..=1, 1..=1),
            symbol.clone(),
            Visibility::Private,
            MethodKind::Instance,
        );

        bar_mod.new_symbol(&mut state.db, symbol, Symbol::Method(foo));

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(3, 3));
    }

    #[test]
    fn test_import_symbol_from_another_module() {
        let symbol = "fizz".to_string();
        let mut state = State::new(Config::new());
        let mut modules = vec![
            hir_module(
                &mut state,
                ModuleName::new("foo"),
                vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                    source: vec![hir::Identifier {
                        name: "fizz".to_string(),
                        location: cols(1, 1),
                    }],
                    symbols: vec![hir::ImportSymbol {
                        name: hir::Identifier {
                            name: symbol.clone(),
                            location: cols(4, 4),
                        },
                        import_as: hir::Identifier {
                            name: symbol.clone(),
                            location: cols(1, 1),
                        },
                        location: cols(1, 1),
                    }],
                    location: cols(1, 1),
                }))],
            ),
            hir_module(
                &mut state,
                ModuleName::new("bar"),
                vec![hir::TopLevelExpression::Import(Box::new(hir::Import {
                    source: vec![hir::Identifier {
                        name: "foo".to_string(),
                        location: cols(1, 1),
                    }],
                    symbols: vec![hir::ImportSymbol {
                        name: hir::Identifier {
                            name: symbol.clone(),
                            location: cols(4, 4),
                        },
                        import_as: hir::Identifier {
                            name: symbol.clone(),
                            location: cols(1, 1),
                        },
                        location: cols(1, 1),
                    }],
                    location: cols(1, 1),
                }))],
            ),
        ];

        let fizz_mod = Module::alloc(
            &mut state.db,
            ModuleName::new("fizz"),
            "fizz.inko".into(),
        );

        let fizz = Method::alloc(
            &mut state.db,
            fizz_mod,
            Location::new(1..=1, 1..=1),
            symbol.clone(),
            Visibility::Public,
            MethodKind::Instance,
        );

        fizz_mod.new_symbol(&mut state.db, symbol, Symbol::Method(fizz));

        assert!(!DefineImportedTypes::run_all(&mut state, &mut modules));

        let error = state.diagnostics.iter().next().unwrap();

        assert_eq!(error.id(), DiagnosticId::InvalidSymbol);
        assert_eq!(error.file(), &PathBuf::from("test.inko"));
        assert_eq!(error.location(), &cols(4, 4));
    }
}
