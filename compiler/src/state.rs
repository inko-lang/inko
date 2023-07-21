//! Compiler state accessible to compiler passes.
use crate::config::{Config, SOURCE, TESTS};
use crate::diagnostics::Diagnostics;
use crate::pkg::manifest::{Manifest, MANIFEST_FILE};
use crate::target::{OperatingSystem, Target};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use types::module_name::ModuleName;
use types::Database;

pub(crate) struct BuildTags {
    values: HashSet<String>,
}

impl BuildTags {
    fn new(target: &Target) -> BuildTags {
        let mut values = HashSet::new();

        values.insert(target.arch_name().to_string());
        values.insert(target.os_name().to_string());
        values.insert(target.abi_name().to_string());

        match target.os {
            OperatingSystem::Freebsd => {
                values.insert("bsd".to_string());
                values.insert("unix".to_string());
            }
            OperatingSystem::Linux | OperatingSystem::Mac => {
                values.insert("unix".to_string());
            }
        }

        BuildTags { values }
    }

    pub(crate) fn is_defined(&self, name: &str) -> bool {
        self.values.contains(name)
    }
}

pub(crate) struct Packages {
    /// A cache mapping module source paths to their project directories.
    ///
    /// If the mapped value is a None, it means no project directory is found.
    roots: HashMap<PathBuf, Option<PathBuf>>,

    /// A mapping of project roots to a mapping of dependency names with their
    /// source directories.
    sources: HashMap<PathBuf, HashMap<String, PathBuf>>,
}

impl Packages {
    fn new() -> Packages {
        Packages { roots: HashMap::new(), sources: HashMap::new() }
    }

    /// Returns the project root directory of the given source file path.
    fn root(&mut self, path: &Path) -> Option<PathBuf> {
        self.roots
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let mut root = path.to_path_buf();

                // "Walk" up the path until we end up with a path that points to
                // the directory the src/ or test/ directory resides in, i.e.
                // the project root.
                loop {
                    if root.ends_with(SOURCE) {
                        if root.pop() {
                            break;
                        } else {
                            return None;
                        }
                    } else if root.ends_with(TESTS) {
                        // We don't want some random /test/ component (e.g.
                        // src/foo/test/bla.inko) to trick us into using the
                        // wrong directory as the project root.
                        match root.pop() {
                            true if root.join(SOURCE).is_dir() => break,
                            true => {}
                            false => return None,
                        }
                    } else if !root.pop() {
                        return None;
                    }
                }

                Some(root)
            })
            .clone()
    }

    /// Returns the path to the src/ directory of the package that contains the
    /// given module.
    fn source_directory(
        &mut self,
        dependencies: &Path,
        root: &PathBuf,
        module: &ModuleName,
    ) -> Option<PathBuf> {
        let head = module.head();

        if let Some(map) = self.sources.get(root) {
            return map.get(head).cloned();
        }

        let manifest_path = root.join(MANIFEST_FILE);

        if !manifest_path.is_file() {
            return None;
        }

        let mut map = HashMap::new();
        let mut found = None;

        // At this stage we expect the manifests to be valid.
        for dep in Manifest::load(&manifest_path).ok()?.into_dependencies() {
            let dir = dependencies
                .join(dep.url.directory_name())
                .join(&format!("v{}", dep.version.major))
                .join(SOURCE);

            if dep.name == head && found.is_none() {
                found = Some(dir.clone());
            }

            map.insert(dep.name, dir);
        }

        self.sources.insert(root.clone(), map);
        found
    }
}

/// A type that caches the result of `PathBuf::is_file()` per path.
struct Exists {
    mapping: HashMap<PathBuf, bool>,
}

impl Exists {
    fn new() -> Exists {
        Exists { mapping: HashMap::new() }
    }

    fn check(&mut self, path: PathBuf) -> Option<PathBuf> {
        match self.mapping.get(&path) {
            Some(true) => Some(path),
            Some(false) => None,
            None if path.is_file() => {
                self.mapping.insert(path.clone(), true);
                Some(path)
            }
            None => {
                self.mapping.insert(path.clone(), false);
                None
            }
        }
    }
}

/// State that is accessible by the compiler passes.
///
/// This is stored in a separate type/module so we don't end up with a circular
/// dependency between a compiler and its passes.
pub(crate) struct State {
    pub(crate) config: Config,
    pub(crate) diagnostics: Diagnostics,
    pub(crate) db: Database,
    pub(crate) build_tags: BuildTags,
    pub(crate) libraries: HashSet<String>,
    packages: Packages,
    exists: Exists,
}

impl State {
    pub(crate) fn new(config: Config) -> Self {
        let diagnostics = Diagnostics::new();
        let db = Database::new();
        let build_tags = BuildTags::new(&config.target);

        Self {
            config,
            diagnostics,
            db,
            build_tags,
            libraries: HashSet::new(),
            packages: Packages::new(),
            exists: Exists::new(),
        }
    }

    pub(crate) fn module_path(
        &mut self,
        importing: PathBuf,
        module: &ModuleName,
    ) -> Option<PathBuf> {
        let rel = module.to_path();

        // If the importing module doesn't have a project root, there's nothing
        // we can do.
        if let Some(root) = self.packages.root(&importing) {
            if let Some(p) = self.exists.check(root.join(SOURCE).join(&rel)) {
                return Some(p);
            }

            if let Some(p) = self
                .packages
                .source_directory(&self.config.dependencies, &root, module)
                .and_then(|src| self.exists.check(src.join(&rel)))
            {
                return Some(p);
            }
        }

        // Additional source directories come last. This way whatever paths are
        // added won't override project-local imports, or imports of third-party
        // packages.
        for dir in &self.config.sources {
            let path = dir.join(&rel);

            if let Some(p) = self.exists.check(path) {
                return Some(p);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::{Abi, Architecture, Target};

    #[test]
    fn test_build_tags() {
        let linux = BuildTags::new(&Target {
            arch: Architecture::Amd64,
            os: OperatingSystem::Linux,
            abi: Abi::Native,
        });

        let bsd = BuildTags::new(&Target {
            arch: Architecture::Amd64,
            os: OperatingSystem::Freebsd,
            abi: Abi::Native,
        });

        let mac = BuildTags::new(&Target {
            arch: Architecture::Amd64,
            os: OperatingSystem::Mac,
            abi: Abi::Native,
        });

        assert!(linux.is_defined("amd64"));
        assert!(linux.is_defined("linux"));
        assert!(linux.is_defined("unix"));
        assert!(linux.is_defined("gnu"));
        assert!(!linux.is_defined("bsd"));

        assert!(bsd.is_defined("amd64"));
        assert!(bsd.is_defined("bsd"));
        assert!(bsd.is_defined("unix"));
        assert!(!bsd.is_defined("linux"));

        assert!(mac.is_defined("amd64"));
        assert!(mac.is_defined("mac"));
        assert!(mac.is_defined("unix"));
        assert!(!mac.is_defined("bsd"));
        assert!(!mac.is_defined("linux"));
    }
}
