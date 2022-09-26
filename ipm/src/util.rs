use crate::error::Error;
use getopts::Options;
use std::env;
use std::fs::{copy, create_dir_all, read_dir};
use std::path::{Path, PathBuf};

const DIR_NAME: &str = "ipm";

/// The directory to install dependencies into.
pub(crate) const DEP_DIR: &str = "dep";

fn windows_local_appdata() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join("AppData").join("Local")))
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    };

    var.filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub(crate) fn usage(options: &Options, summary: &str) {
    let out = options.usage_with_format(|opts| {
        format!(
            "{}\n\nOptions:\n\n{}",
            summary,
            opts.collect::<Vec<String>>().join("\n")
        )
    });

    println!("{}", out);
}

pub(crate) fn data_dir() -> Result<PathBuf, Error> {
    let base = if cfg!(windows) {
        windows_local_appdata()
    } else if cfg!(macos) {
        home_dir().map(|h| h.join("Library").join("Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local").join("share")))
    };

    base.map(|p| p.join(DIR_NAME))
        .ok_or_else(|| error!("No data directory could be determined"))
}

pub(crate) fn cp_r(source: &Path, target: &Path) -> Result<(), Error> {
    create_dir_all(target)?;

    let mut pending = vec![source.to_path_buf()];

    while let Some(path) = pending.pop() {
        let entries = read_dir(&path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
                continue;
            }

            let rel = path.strip_prefix(&source).unwrap();
            let target = target.join(rel);
            let dir = target.parent().unwrap();

            create_dir_all(&dir)
                .map_err(|e| error!("Failed to create {:?}: {}", dir, e))?;

            if target.is_file() {
                fail!(
                    "Failed to copy {} to {} as the target file already exists",
                    path.display(),
                    target.display()
                );
            }

            copy(&path, &target).map_err(|error| {
                error!(
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

pub(crate) fn red<S: Into<String>>(message: S) -> String {
    if cfg!(windows) {
        message.into()
    } else {
        format!("\x1b[1m\x1b[31m{}\x1b[0m\x1b[0m", message.into())
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
        let temp = env::temp_dir().join("ipm-test_cp_r");

        assert!(cp_r(&src.join("src"), &temp).is_ok());
        assert!(temp.join("util.rs").is_file());

        remove_dir_all(temp).unwrap();
    }
}
