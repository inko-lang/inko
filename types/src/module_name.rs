//! Types to represent module names.
use std::fmt;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

const MAIN_MODULE: &str = "main";
const SOURCE_EXT: &str = "inko";
pub const SEPARATOR: &str = ".";

/// The fully qualified name of a module.
#[derive(Eq, PartialEq, Hash, Clone, Ord, PartialOrd)]
pub struct ModuleName {
    value: String,
}

impl ModuleName {
    pub fn from_relative_path(path: &Path) -> Self {
        Self::new(
            path.with_extension("")
                .to_string_lossy()
                .replace(MAIN_SEPARATOR, SEPARATOR),
        )
    }

    pub fn main() -> Self {
        Self::new(MAIN_MODULE.to_string())
    }

    pub fn std_init() -> Self {
        Self::new("std.init")
    }

    pub fn new<S: Into<String>>(value: S) -> Self {
        Self { value: value.into() }
    }

    pub fn is_std(&self) -> bool {
        self.value.starts_with("std.")
    }

    pub fn head(&self) -> &str {
        self.value.split(SEPARATOR).next().unwrap()
    }

    pub fn tail(&self) -> &str {
        self.value.split(SEPARATOR).last().unwrap()
    }

    pub fn to_path(&self) -> PathBuf {
        let mut path = PathBuf::from(
            self.value.replace(SEPARATOR, &MAIN_SEPARATOR.to_string()),
        );

        path.set_extension(SOURCE_EXT);
        path
    }

    pub fn normalized_name(&self) -> String {
        self.value.replace(SEPARATOR, "_")
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
}

impl From<Vec<String>> for ModuleName {
    fn from(values: Vec<String>) -> Self {
        Self { value: values.join(SEPARATOR) }
    }
}

impl fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Debug for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ModuleName({})", self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR;

    #[test]
    fn test_from_relative_path() {
        let path = PathBuf::from("foo").join("bar.inko");
        let name = ModuleName::from_relative_path(&path);

        assert_eq!(name, ModuleName::new("foo.bar"));
    }

    #[test]
    fn test_main() {
        assert_eq!(ModuleName::main(), ModuleName::new("main"));
    }

    #[test]
    fn test_to_path() {
        let name = ModuleName::new("foo.bar");
        let path = name.to_path();

        assert_eq!(
            path,
            PathBuf::from(format!("foo{}bar.inko", MAIN_SEPARATOR))
        );
    }

    #[test]
    fn test_is_std() {
        let name1 = ModuleName::new("foo.bar");
        let name2 = ModuleName::new("std.bar");

        assert!(!name1.is_std());
        assert!(name2.is_std());
    }

    #[test]
    fn test_head() {
        let name = ModuleName::new("foo.bar");

        assert_eq!(name.head(), &"foo".to_string());
    }

    #[test]
    fn test_tail() {
        let name = ModuleName::new("foo.bar");

        assert_eq!(name.tail(), &"bar".to_string());
    }

    #[test]
    fn test_display() {
        let name = ModuleName::new("foo.bar");

        assert_eq!(format!("{}", name), "foo.bar".to_string());
    }

    #[test]
    fn test_debug() {
        let name = ModuleName::new("foo.bar");

        assert_eq!(format!("{:?}", name), "ModuleName(foo.bar)".to_string());
    }

    #[test]
    fn test_normalized_name() {
        assert_eq!(
            ModuleName::new("std.foo.bar").normalized_name(),
            "std_foo_bar"
        );
    }
}
