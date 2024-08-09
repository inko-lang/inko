use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REMOTE: &str = "origin";

pub(crate) struct Repository {
    /// The path to the local clone of the repository.
    pub(crate) path: PathBuf,
}

pub(crate) struct Tag {
    /// The SHA of the commit the tag points to.
    pub(crate) target: String,
}

impl Repository {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        if path.is_dir() {
            Ok(Self { path: path.to_path_buf() })
        } else {
            Err(format!(
                "The Git repository at {} doesn't exist",
                path.display()
            ))
        }
    }

    pub(crate) fn clone(
        url: &str,
        path: &Path,
        branch: &str,
    ) -> Result<Self, String> {
        run(
            "clone",
            None,
            &[
                OsStr::new("--single-branch"),
                OsStr::new("--branch"),
                OsStr::new(branch),
                OsStr::new(url),
                path.as_os_str(),
            ],
        )
        .map_err(|err| format!("Failed to clone {}: {}", url, err))?;

        Ok(Self { path: path.to_path_buf() })
    }

    pub(crate) fn fetch(&mut self) -> Result<(), String> {
        run(
            "fetch",
            Some(self.path.as_path()),
            &[OsStr::new(REMOTE), OsStr::new("--tags")],
        )
        .map_err(|err| {
            format!("Failed to update {}: {}", self.path.display(), err)
        })?;

        Ok(())
    }

    pub(crate) fn tag(&self, name: &str) -> Option<Tag> {
        run(
            "rev-list",
            Some(self.path.as_path()),
            &[OsStr::new("-n"), OsStr::new("1"), OsStr::new(name)],
        )
        .ok()
        .map(|output| Tag { target: output.trim().to_string() })
    }

    pub(crate) fn version_tag_names(&self) -> Vec<String> {
        if let Ok(output) = run(
            "tag",
            Some(self.path.as_path()),
            &[OsStr::new("-l"), OsStr::new("v*")],
        ) {
            output.split('\n').map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn checkout(&self, name: &str) -> Result<(), String> {
        run("checkout", Some(self.path.as_path()), &[OsStr::new(name)])?;
        Ok(())
    }
}

fn run(
    command: &str,
    working_directory: Option<&Path>,
    arguments: &[&OsStr],
) -> Result<String, String> {
    let mut cmd = Command::new("git");

    cmd.arg(command);
    cmd.args(arguments);

    if let Some(path) = working_directory {
        cmd.current_dir(path);
    }

    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|err| format!("Failed to spawn 'git {}': {}", command, err))?;
    let output = child.wait_with_output().map_err(|err| format!("{}", err))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .into_owned()
            .trim()
            .to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .into_owned()
            .trim()
            .to_string())
    }
}
