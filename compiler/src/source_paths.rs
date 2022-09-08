//! Types for storing source paths and finding modules in these paths.
use std::collections::HashMap;
use std::path::PathBuf;

/// A collection of paths to search for source modules.
pub struct SourcePaths {
    /// All the paths that are available.
    paths: Vec<PathBuf>,

    /// A cache that maps relative paths to their absolute source paths.
    cache: HashMap<PathBuf, PathBuf>,
}

impl SourcePaths {
    pub fn new() -> Self {
        Self { paths: Vec::new(), cache: HashMap::new() }
    }

    pub fn add(&mut self, path: PathBuf) {
        self.paths.push(path.canonicalize().unwrap_or(path));
    }

    pub fn get(&mut self, relative: &PathBuf) -> Option<PathBuf> {
        let cached = self.cache.get(relative);

        if cached.is_some() {
            return cached.cloned();
        }

        for dir in &self.paths {
            let abs_path = dir.join(relative);

            if abs_path.is_file() {
                self.cache.insert(relative.clone(), abs_path.clone());

                return Some(abs_path);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::{remove_file, write};

    #[test]
    fn test_get() {
        let mut paths = SourcePaths::new();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        paths.add(root.clone());

        assert_eq!(paths.get(&PathBuf::from("kittens")), None);

        assert_eq!(
            paths.get(&PathBuf::from("Cargo.toml")),
            Some(root.join("Cargo.toml").canonicalize().unwrap())
        );

        // Test again to ensure the cache uses the correct output.
        assert_eq!(
            paths.get(&PathBuf::from("Cargo.toml")),
            Some(root.join("Cargo.toml").canonicalize().unwrap())
        );
    }

    #[test]
    fn test_get_with_no_longer_existing_file() {
        let mut paths = SourcePaths::new();
        let root = temp_dir().canonicalize().unwrap();
        let name = "inko_compiler_test.txt";
        let temp = root.join(name);

        paths.add(root);
        write(&temp, Vec::new()).unwrap();

        assert_eq!(
            paths.get(&PathBuf::from(name)),
            Some(temp.canonicalize().unwrap())
        );

        remove_file(&temp).unwrap();

        // The file is cached, so we still return it. This way lets us test if
        // caching works, without setting hard expectations on how things are
        // cached.
        assert_eq!(paths.get(&PathBuf::from(name)), Some(temp));
    }
}
