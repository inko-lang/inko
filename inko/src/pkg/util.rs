use compiler::config;
use std::fs::{copy, create_dir_all, read_dir};
use std::path::{Path, PathBuf};

pub(crate) fn data_dir() -> Result<PathBuf, String> {
    config::data_directory()
        .map(|p| p.join("packages"))
        .ok_or_else(|| "No data directory could be determined".to_string())
}

pub(crate) fn cp_r(source: &Path, target: &Path) -> Result<(), String> {
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
