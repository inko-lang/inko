use crate::pkg::version::Version;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REMOTE: &str = "origin";

pub struct Repository {
    /// The path to the local clone of the repository.
    pub path: PathBuf,
}

pub struct Tag {
    /// The SHA of the commit the tag points to.
    pub target: String,
}

impl Repository {
    pub fn open(path: &Path) -> Result<Self, String> {
        if path.is_dir() {
            Ok(Self { path: path.to_path_buf() })
        } else {
            Err(format!(
                "the Git repository at {} doesn't exist",
                path.display()
            ))
        }
    }

    pub fn open_and_fetch(path: &Path) -> Result<Self, String> {
        let mut repo = Self::open(path)?;

        repo.fetch()?;
        Ok(repo)
    }

    pub fn full_clone(url: &str, path: &Path) -> Result<Self, String> {
        run("clone", None, &[OsStr::new(url), path.as_os_str()])
            .map_err(|err| format!("failed to clone {}: {}", url, err))?;

        Ok(Self { path: path.to_path_buf() })
    }

    pub fn partial_clone(
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
        .map_err(|err| format!("failed to clone {}: {}", url, err))?;

        Ok(Self { path: path.to_path_buf() })
    }

    pub fn fetch(&mut self) -> Result<(), String> {
        run(
            "fetch",
            Some(self.path.as_path()),
            &[OsStr::new(REMOTE), OsStr::new("--tags")],
        )
        .map_err(|err| {
            format!("failed to update {}: {}", self.path.display(), err)
        })?;

        Ok(())
    }

    pub fn tag(&self, name: &str) -> Option<Tag> {
        run(
            "rev-list",
            Some(self.path.as_path()),
            &[OsStr::new("-n"), OsStr::new("1"), OsStr::new(name)],
        )
        .ok()
        .map(|output| Tag { target: output.trim().to_string() })
    }

    pub fn version_tag_names(&self) -> Vec<String> {
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

    pub fn latest_version(&self) -> Option<Version> {
        let mut latest = None;

        for tag in self.version_tag_names() {
            let ver = Version::parse(&tag[1..]);

            if ver.is_some() && ver > latest {
                latest = ver;
            }
        }

        latest
    }

    pub fn checkout(&self, name: &str) -> Result<(), String> {
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
        .map_err(|err| format!("failed to spawn 'git {}': {}", command, err))?;
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
