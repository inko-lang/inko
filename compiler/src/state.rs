//! Compiler state accessible to compiler passes.
use crate::config::Config;
use crate::diagnostics::Diagnostics;
use crate::target::{OperatingSystem, Target};
use std::collections::HashSet;
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
            OperatingSystem::Freebsd | OperatingSystem::Openbsd => {
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

/// State that is accessible by the compiler passes.
///
/// This is stored in a separate type/module so we don't end up with a circular
/// dependency between a compiler and its passes.
pub(crate) struct State {
    pub(crate) config: Config,
    pub(crate) diagnostics: Diagnostics,
    pub(crate) db: Database,
    pub(crate) build_tags: BuildTags,

    /// The C libraries to import.
    pub(crate) libraries: HashSet<String>,
}

impl State {
    pub(crate) fn new(config: Config) -> Self {
        let diagnostics = Diagnostics::new();
        let db = Database::new();
        let build_tags = BuildTags::new(&config.target);

        Self { config, diagnostics, db, build_tags, libraries: HashSet::new() }
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
