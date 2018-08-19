//! Module for obtaining paths to directories, such as the home directory.
use dirs;
use std::env;
use std::io;

/// Returns the path to the home directory.
pub fn home() -> Option<String> {
    dirs::home_dir().map(|path| path.to_string_lossy().into_owned())
}

/// Returns the path to the temporary directory.
pub fn temp() -> String {
    env::temp_dir().to_string_lossy().into_owned()
}

/// Returns the current working directory.
pub fn working_directory() -> io::Result<String> {
    env::current_dir().map(|path| path.to_string_lossy().into_owned())
}

/// Returns the current working directory.
pub fn set_working_directory(directory: &String) -> io::Result<()> {
    env::set_current_dir(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_home() {
        if let Some(path) = home() {
            if !path.is_empty() {
                assert!(Path::new(&path).is_dir());
            }
        }
    }

    #[test]
    fn test_temp() {
        assert!(Path::new(&temp()).is_dir());
    }

    #[test]
    fn test_working_directory() {
        let dir_res = working_directory();

        assert!(dir_res.is_ok());
        assert!(Path::new(&dir_res.unwrap()).is_dir());
    }

    #[test]
    fn test_set_working_directory() {
        let current_dir = working_directory().unwrap();

        assert!(set_working_directory(&current_dir).is_ok());
    }
}
