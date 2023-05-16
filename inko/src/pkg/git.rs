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

    pub(crate) fn clone(url: &str, path: &Path) -> Result<Self, String> {
        run("clone", None, &[OsStr::new(url), path.as_os_str()])
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_dir_all;

    fn create_tag(repo: &mut Repository, name: &str) {
        run("tag", Some(repo.path.as_path()), &[OsStr::new(name)]).unwrap();
    }

    fn source_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn temp_dir() -> PathBuf {
        source_dir().join("tmp")
    }

    #[test]
    fn test_repository_open() {
        let repo = Repository::open(&source_dir());

        assert!(repo.is_ok());
    }

    #[test]
    fn test_repository_clone() {
        let temp = temp_dir().join("inko-pkg-test_repository_clone");
        let repo = Repository::clone(source_dir().to_str().unwrap(), &temp);

        assert!(repo.is_ok());
        assert!(temp.is_dir());

        remove_dir_all(temp).unwrap();
    }

    #[test]
    fn test_repository_fetch() {
        let temp = temp_dir().join("inko-pkg-test_repository_fetch");
        let mut repo =
            Repository::clone(source_dir().to_str().unwrap(), &temp).unwrap();

        assert!(repo.fetch().is_ok());

        remove_dir_all(temp).unwrap();
    }

    #[test]
    fn test_repository_tag() {
        let temp = temp_dir().join("inko-pkg-test_repository_tag");
        let mut repo =
            Repository::clone(source_dir().to_str().unwrap(), &temp).unwrap();

        assert!(repo.tag("test").is_none());
        create_tag(&mut repo, "test");
        assert!(repo.tag("test").is_some());

        remove_dir_all(temp).unwrap();
    }

    #[test]
    fn test_repository_checkout() {
        let temp = temp_dir().join("inko-pkg-test_repository_checkout");
        let mut repo =
            Repository::clone(source_dir().to_str().unwrap(), &temp).unwrap();

        create_tag(&mut repo, "test");

        let tag = repo.tag("test").unwrap();

        assert!(repo.checkout(&tag.target).is_ok());

        remove_dir_all(temp).unwrap();
    }

    #[test]
    fn test_repository_version_tag_names() {
        let temp =
            temp_dir().join("inko-pkg-test_repository_version_tag_names");
        let mut repo =
            Repository::clone(source_dir().to_str().unwrap(), &temp).unwrap();

        create_tag(&mut repo, "v999.0.0");
        create_tag(&mut repo, "v999.0.1");

        let names = repo.version_tag_names();

        assert!(names.contains(&"v999.0.0".to_string()));
        assert!(names.contains(&"v999.0.1".to_string()));

        remove_dir_all(temp).unwrap();
    }
}
