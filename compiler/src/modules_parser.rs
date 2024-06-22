//! Parsing of Inko source code into ASTs.
use crate::diagnostics::DiagnosticId;
use crate::state::{BuildTags, State};
use ast::nodes::{Module, Node, TopLevelExpression};
use ast::parser::Parser;
use ast::source_location::SourceLocation;
use std::collections::{HashMap, HashSet};
use std::fs::read;
use std::path::PathBuf;
use types::module_name::ModuleName;

fn imported_modules(
    module: &mut Module,
    tags: &BuildTags,
) -> Vec<(ModuleName, SourceLocation)> {
    let mut names = Vec::new();

    for expr in &mut module.expressions {
        let (path, loc) = match expr {
            TopLevelExpression::Import(ref mut node) => {
                node.include = node.tags.as_ref().map_or(true, |n| {
                    n.values.iter().all(|i| tags.is_defined(&i.name))
                });

                if !node.include {
                    continue;
                }

                (&node.path, node.location().clone())
            }
            _ => continue,
        };

        let name = ModuleName::from(
            path.steps.iter().map(|i| i.name.clone()).collect::<Vec<_>>(),
        );

        names.push((name, loc));
    }

    names
}

/// A parsed module and the modules it depends on.
pub(crate) struct ParsedModule {
    pub(crate) name: ModuleName,
    pub(crate) ast: Module,
}

/// A compiler pass for parsing all the modules into an AST.
pub(crate) struct ModulesParser<'a> {
    state: &'a mut State,

    /// If parsing of comments is to be enabled or not.
    comments: bool,
}

impl<'a> ModulesParser<'a> {
    pub(crate) fn new(state: &'a mut State) -> Self {
        Self { state, comments: false }
    }

    pub(crate) fn with_documentation_comments(state: &'a mut State) -> Self {
        Self { state, comments: true }
    }

    /// Parses an initial set of modules and all their dependencies.
    ///
    /// Modules are parsed in a depth-first order. That is, given these imports:
    ///
    ///     import foo
    ///     import bar
    ///
    /// We first parse the surrounding module, then `foo`, then `bar`.
    pub(crate) fn run(
        &mut self,
        initial: Vec<(ModuleName, PathBuf)>,
    ) -> Vec<ParsedModule> {
        let mut scheduled = HashSet::new();
        let mut modules = HashMap::new();
        let mut pending = initial;

        for (_, path) in &pending {
            scheduled.insert(path.clone());
        }

        let init = &self.state.config.init_module;
        let init_id = self.state.dependency_graph.add_module(init.clone());

        {
            let path = self.state.config.std.join(init.to_path());

            scheduled.insert(path.clone());
            pending.push((init.clone(), path));
        }

        while let Some((qname, file)) = pending.pop() {
            if let Some(mut ast) = self.parse(&file) {
                let deps = imported_modules(&mut ast, &self.state.build_tags);
                let depending_id =
                    self.state.dependency_graph.add_module(qname.clone());

                self.state
                    .dependency_graph
                    .add_depending(init_id, depending_id);

                modules
                    .insert(qname.clone(), ParsedModule { name: qname, ast });

                for (dep, location) in deps {
                    let path = if let Some(val) =
                        self.state.module_path(file.clone(), &dep)
                    {
                        val
                    } else {
                        self.state.diagnostics.error(
                            DiagnosticId::InvalidFile,
                            format!("the module '{}' couldn't be found", dep),
                            file.clone(),
                            location,
                        );

                        continue;
                    };

                    let dependency_id =
                        self.state.dependency_graph.add_module(dep.clone());

                    self.state
                        .dependency_graph
                        .add_depending(dependency_id, depending_id);

                    if scheduled.contains(&path) {
                        continue;
                    }

                    scheduled.insert(path.clone());
                    pending.push((dep, path));
                }
            }
        }

        let mut result: Vec<ParsedModule> = modules.into_values().collect();

        // We sort the modules so we process them in a deterministic order,
        // resulting in diagnostics being produced in a deterministic order.
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    fn parse(&mut self, file: &PathBuf) -> Option<Module> {
        let input = match read(file) {
            Ok(result) => result,
            Err(e) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidFile,
                    e.to_string(),
                    file.clone(),
                    SourceLocation::new(1..=1, 1..=1),
                );

                return None;
            }
        };

        let mut parser = if self.comments {
            Parser::with_comments(input, file.clone())
        } else {
            Parser::new(input, file.clone())
        };

        match parser.parse() {
            Ok(ast) => Some(ast),
            Err(err) => {
                self.state.diagnostics.error(
                    DiagnosticId::InvalidSyntax,
                    err.message,
                    file.clone(),
                    err.location,
                );

                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::env::temp_dir;
    use std::fs::{remove_file, write};

    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        fn new(name: &str) -> Self {
            Self { path: temp_dir().join(format!("{}.inko", name)) }
        }

        fn path(&self) -> &PathBuf {
            &self.path
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = remove_file(&self.path);
        }
    }

    #[test]
    fn test_run_with_existing_modules() {
        let file1 = TempFile::new("parsing1a");
        let file2 = TempFile::new("parsing2a");
        let file3 = TempFile::new("inita");

        write(file1.path(), "import parsing2a").unwrap();
        write(file2.path(), "let A = 10").unwrap();
        write(file3.path(), "").unwrap();

        let mut state = State::new(Config::new());

        state.config.std = temp_dir();
        state.config.add_source_directory(temp_dir());
        state.config.init_module = ModuleName::new("inita");

        let mut pass = ModulesParser::new(&mut state);
        let mods = pass.run(vec![(ModuleName::main(), file1.path().clone())]);

        assert_eq!(mods.len(), 3);

        let names = mods.iter().map(|m| m.name.clone()).collect::<Vec<_>>();

        assert!(names.contains(&ModuleName::main()));
        assert!(names.contains(&ModuleName::new("parsing2a")));
        assert_eq!(state.diagnostics.iter().count(), 0);
    }

    #[test]
    fn test_run_with_syntax_error() {
        let file1 = TempFile::new("parsing1b");
        let file2 = TempFile::new("parsing2b");
        let file3 = TempFile::new("initb");

        write(file1.path(), "import parsing2b").unwrap();
        write(file2.path(), "10").unwrap();
        write(file3.path(), "").unwrap();

        let mut state = State::new(Config::new());

        state.config.add_source_directory(temp_dir());
        state.config.std = temp_dir();
        state.config.init_module = ModuleName::new("initb");

        let mut pass = ModulesParser::new(&mut state);
        let mods = pass.run(vec![(ModuleName::main(), file1.path().clone())]);

        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, ModuleName::new("initb"));
        assert_eq!(mods[1].name, ModuleName::main());
        assert_eq!(state.diagnostics.iter().count(), 1);
    }

    #[test]
    fn test_run_with_missing_file() {
        let file1 = TempFile::new("parsing1c");
        let file2 = TempFile::new("initc");

        write(file1.path(), "import parsing2c").unwrap();
        write(file2.path(), "").unwrap();

        let mut state = State::new(Config::new());

        state.config.add_source_directory(temp_dir());
        state.config.std = temp_dir();
        state.config.init_module = ModuleName::new("initc");

        let mut pass = ModulesParser::new(&mut state);
        let mods = pass.run(vec![(ModuleName::main(), file1.path().clone())]);

        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, ModuleName::new("initc"));
        assert_eq!(mods[1].name, ModuleName::main());
        assert_eq!(state.diagnostics.iter().count(), 1);
    }
}
