use crate::config;
use crate::file_lock::FileLock;
use std::fmt::Display;
use std::fs::{copy, create_dir_all, read_dir};
use std::io::{IsTerminal, Write, stderr};
use std::path::{Path, PathBuf};

/// Returns the path to the data directory and its exclusive access lock.
///
/// Access to the data directory is guarded using a lock so concurrent processes
/// (e.g. an `inko pkg sync` and an `inko pkg add`) won't interfere with each
/// other.
pub fn data_dir() -> Result<(PathBuf, FileLock), String> {
    let path = config::data_directory()
        .map(|p| p.join("packages"))
        .ok_or_else(|| "no data directory could be determined".to_string())?;

    create_dir_all(&path)
        .map_err(|e| format!("failed to create {}: {}", path.display(), e))?;

    let lock = FileLock::new(&path.join("lock"))
        .map_err(|e| format!("failed to get the packages lock: {}", e))?;

    Ok((path, lock))
}

pub fn cp_r(source: &Path, target: &Path) -> Result<(), String> {
    create_dir_all(target).map_err(|e| e.to_string())?;

    let mut pending = vec![source.to_path_buf()];

    while let Some(path) = pending.pop() {
        let entries = read_dir(&path).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
                continue;
            }

            let rel = path.strip_prefix(source).unwrap();
            let target = target.join(rel);
            let dir = target.parent().unwrap();

            create_dir_all(dir)
                .map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;

            if target.is_file() {
                return Err(format!(
                    "Failed to copy {} to {} as the target file already exists",
                    path.display(),
                    target.display()
                ));
            }

            copy(&path, &target).map_err(|error| {
                format!(
                    "Failed to copy {} to {}: {}",
                    path.to_string_lossy(),
                    target.to_string_lossy(),
                    error
                )
            })?;
        }
    }

    Ok(())
}

pub struct Status {
    wrote: bool,
    color: bool,
}

impl Status {
    pub fn new() -> Self {
        Self { wrote: false, color: stderr().is_terminal() }
    }

    pub fn set<T: Display>(&mut self, title: &T, msg: &str) {
        let mut out = stderr();

        if self.color {
            self.wrote = true;

            let _ = write!(out, "\x1B[2K\r\x1B[1m{}\x1B[0m: {}", title, msg);
            let _ = out.flush();
        } else {
            let _ = writeln!(out, "{}: {}", title, msg);
        };
    }

    pub fn reset(&mut self) {
        let mut out = stderr();

        if self.color && self.wrote {
            self.wrote = false;

            let _ = write!(out, "\x1B[2K\r");
            let _ = out.flush();
        }
    }
}

impl Drop for Status {
    fn drop(&mut self) {
        if self.wrote {
            let mut out = stderr();
            let _ = writeln!(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs::remove_dir_all;

    #[test]
    fn test_cp_r() {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let temp = env::temp_dir().join("inko-pkg-test_cp_r");

        assert!(cp_r(&src.join("src"), &temp).is_ok());
        assert!(temp.join("pkg").join("util.rs").is_file());

        remove_dir_all(temp).unwrap();
    }
}
