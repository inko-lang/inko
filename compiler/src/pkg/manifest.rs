use crate::config::Config;
use crate::pkg::version::Version;
use blake3;
use std::fmt;
use std::fs::{read_dir, File, OpenOptions};
use std::io::{BufRead, BufReader, Error, Read, Write};
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "inko.pkg";

fn dependency_manifests(config: &Config) -> Result<Vec<PathBuf>, Error> {
    let mut paths = vec![PathBuf::from(MANIFEST_FILE)];

    if !config.dependencies.is_dir() {
        return Ok(paths);
    }

    for entry in read_dir(&config.dependencies)? {
        let entry = entry?;
        let dep_dir = entry.path();

        if !dep_dir.is_dir() {
            continue;
        }

        for entry in read_dir(dep_dir)? {
            let entry = entry?;
            let ver_dir = entry.path();

            if ver_dir.is_dir() {
                paths.push(ver_dir.join(MANIFEST_FILE));
            }
        }
    }

    Ok(paths)
}

/// The URL of a package.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct Url {
    pub value: String,
}

impl Url {
    pub fn parse(input: &str) -> Option<Self> {
        if input.contains(' ') || input.is_empty() {
            return None;
        }

        // GitLab and GitHub URLs will be the most common, so we allow using
        // these URLs in a slightly shorter form, making them a bit easier to
        // work with from the CLI.
        let value = if input.starts_with("gitlab.com")
            || input.starts_with("github.com")
        {
            format!("https://{}", input)
        } else {
            input.to_string()
        };

        Some(Url::new(value))
    }

    pub fn new<S: Into<String>>(value: S) -> Self {
        Self { value: value.into() }
    }

    pub fn directory_name(&self) -> String {
        blake3::hash(self.value.as_bytes()).to_string()
    }

    pub fn import_name(&self) -> String {
        let tail = self.value.split('/').next_back().unwrap();

        // For generic names like "http" or "sqlite3", creating a repository
        // with such a name may be confusing, as one might think it's e.g. a
        // fork of a project, or perhaps the name conflicts with an existing
        // project.
        //
        // To handle that, if a project is called "inko-http", we strip the
        // "inko-" prefix. This way within the code you can just use "http" as
        // the module name.
        if let Some(name) = tail.strip_prefix("inko-") {
            name.to_string()
        } else {
            tail.to_string()
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

/// A Git (SHA1) checksum.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Checksum {
    pub value: String,
}

impl Checksum {
    pub fn parse(input: &str) -> Option<Self> {
        if input.len() != 40 {
            return None;
        }

        Some(Checksum::new(input))
    }

    pub fn new<S: Into<String>>(value: S) -> Self {
        Self { value: value.into() }
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

/// A dependency as specified in the manifest.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Dependency {
    pub url: Url,
    pub name: String,
    pub version: Version,
    pub checksum: Checksum,
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "require {} {} {}", self.url, self.version, self.checksum)
    }
}

#[derive(Eq, PartialEq, Debug)]
pub enum Entry {
    Comment(String),
    Dependency(Dependency),
    EmptyLine,
    InkoVersion(Version),
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Entry::Comment(comment) => write!(f, "#{}", comment),
            Entry::EmptyLine => Ok(()),
            Entry::Dependency(dep) => dep.fmt(f),
            Entry::InkoVersion(version) => {
                write!(f, "require inko {}", version)
            }
        }
    }
}

/// A dependency manifest parsed from a `inko.pkg` file.
#[derive(Eq, PartialEq, Debug)]
pub struct Manifest {
    pub entries: Vec<Entry>,
}

impl Manifest {
    pub fn all(config: &Config) -> Result<Vec<Manifest>, String> {
        let mut manifests = Vec::new();
        let paths = dependency_manifests(config).map_err(|e| {
            format!("failed to read the dependency manifests: {}", e)
        })?;

        for path in paths.into_iter().filter(|v| v.is_file()) {
            let manifest = Manifest::load(&path).map_err(|e| {
                format!(
                    "failed to load the manifest '{}': {}",
                    path.display(),
                    e
                )
            })?;

            manifests.push(manifest);
        }

        Ok(manifests)
    }

    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn load<P: AsRef<Path>>(path: &P) -> Result<Self, String> {
        let path = path.as_ref();

        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| e.to_string())
            .and_then(|mut file| Self::parse(&mut file))
    }

    fn parse<R: Read>(stream: &mut R) -> Result<Self, String> {
        let reader = BufReader::new(stream);
        let mut manifest = Self { entries: Vec::new() };
        let mut inko_req_line_num = 0;

        for (index, line) in reader.lines().enumerate() {
            let lnum = index + 1;
            let line = line.map_err(|err| {
                format!("failed to read lines from the manifest: {}", err)
            })?;

            let trimmed = line.trim();

            if trimmed.is_empty() {
                manifest.entries.push(Entry::EmptyLine);
                continue;
            }

            if let Some(stripped) = trimmed.strip_prefix('#') {
                manifest.entries.push(Entry::Comment(stripped.to_string()));
                continue;
            }

            let chunks: Vec<_> = trimmed.split(' ').collect();

            match chunks[..] {
                [action, ..] if action != "require" => {
                    return Err(format!(
                        "expected line {} to start with 'require', not '{}'",
                        lnum, action
                    ));
                }
                ["require", "inko", version] if inko_req_line_num == 0 => {
                    inko_req_line_num = lnum;

                    let version = Version::parse(version).ok_or_else(|| {
                        format!("the version on line {} is invalid", lnum)
                    })?;

                    manifest.entries.push(Entry::InkoVersion(version));
                }
                ["require", "inko", _] => {
                    return Err(format!(
                        "the Inko version requirement on line {} is invalid \
                        as a requirement is already specified on line {}",
                        lnum, inko_req_line_num,
                    ));
                }
                ["require", url, version, checksum] => {
                    let url = Url::parse(url).ok_or_else(|| {
                        format!("the URI on line {} is invalid", lnum)
                    })?;
                    let name = url.import_name();
                    let version = Version::parse(version).ok_or_else(|| {
                        format!("the version on line {} is invalid", lnum)
                    })?;
                    let checksum =
                        Checksum::parse(checksum).ok_or_else(|| {
                            format!("the checksum on line {} is invalid", lnum)
                        })?;

                    manifest.entries.push(Entry::Dependency(Dependency {
                        url,
                        name,
                        version,
                        checksum,
                    }));
                }
                _ => {
                    return Err(format!(
                        "the entry on line {} is invalid",
                        lnum
                    ));
                }
            }
        }

        Ok(manifest)
    }

    pub fn set_inko_version(&mut self, version: Version) {
        let inko_version = Entry::InkoVersion(version);

        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::InkoVersion(_)))
        {
            *entry = inko_version;
        } else {
            self.entries.push(inko_version);
        }
    }

    pub fn add_dependency(
        &mut self,
        url: Url,
        name: String,
        version: Version,
        checksum: Checksum,
    ) {
        self.entries.push(Entry::Dependency(Dependency {
            url,
            name,
            version,
            checksum,
        }));
    }

    pub fn find_dependency(&mut self, url: &Url) -> Option<&mut Dependency> {
        self.entries.iter_mut().find_map(|entry| match entry {
            Entry::Dependency(dep) if &dep.url == url => Some(dep),
            _ => None,
        })
    }

    pub fn remove_dependency(&mut self, url: &Url) {
        self.entries.retain(
            |val| !matches!(val, Entry::Dependency(dep) if &dep.url == url),
        )
    }

    pub fn dependencies_mut(&mut self) -> Vec<&mut Dependency> {
        self.entries
            .iter_mut()
            .filter_map(|entry| match entry {
                Entry::Dependency(dep) => Some(dep),
                _ => None,
            })
            .collect()
    }

    pub fn into_dependencies(self) -> Vec<Dependency> {
        self.entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Dependency(dep) => Some(dep),
                _ => None,
            })
            .collect()
    }

    pub fn save<P: AsRef<Path>>(&self, path: &P) -> Result<(), String> {
        let path = path.as_ref();

        File::create(path)
            .and_then(|mut file| file.write_all(self.to_string().as_bytes()))
            .map_err(|e| format!("failed to update {}: {}", path.display(), e))
    }

    pub fn minimum_inko_version(&self) -> Option<Version> {
        self.entries.iter().find_map(|e| match e {
            Entry::InkoVersion(v) => Some(v.clone()),
            _ => None,
        })
    }
}

impl fmt::Display for Manifest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for entry in &self.entries {
            writeln!(f, "{}", entry)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_parse() {
        assert_eq!(
            Url::parse("https://gitlab.com/foo/bar"),
            Some(Url::new("https://gitlab.com/foo/bar"))
        );
        assert_eq!(
            Url::parse("gitlab.com/foo/bar"),
            Some(Url::new("https://gitlab.com/foo/bar"))
        );
        assert_eq!(
            Url::parse("https://github.com/foo/bar"),
            Some(Url::new("https://github.com/foo/bar"))
        );
        assert_eq!(
            Url::parse("github.com/foo/bar"),
            Some(Url::new("https://github.com/foo/bar"))
        );
        assert_eq!(
            Url::parse("git@gitlab.com:foo/bar.git"),
            Some(Url::new("git@gitlab.com:foo/bar.git"))
        );

        assert_eq!(Url::parse(""), None);
        assert_eq!(Url::parse("git@gitlab .com:foo/bar.git"), None);
    }

    #[test]
    fn test_url_directory_name() {
        assert_eq!(
            Url::new("https://gitlab.com/foo/bar").directory_name(),
            "6c95f3810d546c9b4137d8291af2abe47019f97b8643ae0800db9da680ce811e"
                .to_string()
        );
        assert_eq!(
            Url::new("http://gitlab.com/foo/bar").directory_name(),
            "6100c5254dd22a9f5577da816f5df90cff0e535e2d7c9fa7356e945d4c364107"
                .to_string()
        );
    }

    #[test]
    fn test_url_import_name() {
        assert_eq!(
            Url::new("https://gitlab.com/foo/bar").import_name(),
            "bar".to_string()
        );
        assert_eq!(
            Url::new("https://gitlab.com/foo/inko-http").import_name(),
            "http".to_string()
        );
    }

    #[test]
    fn test_manifest_parse_invalid() {
        let missing_chunks = "# Ignore me
        require https://gitlab.com/inko-lang/foo 1.2.3";

        let invalid_cmd = "# Ignore me
        bla https://gitlab.com/inko-lang/foo 1.2.3 abcdef123";

        let invalid_version =
            "require https://gitlab.com/inko-lang/foo 1.2 abc";
        let invalid_checksum =
            "require https://gitlab.com/inko-lang/foo 1.2.3 abc";

        let invalid_inko_version = "require inko 1.2";
        let invalid_inko_version_extra_chunk = "require inko 1.2.3 abc";
        let invalid_inko_version_redundant =
            "require inko 1.2.3\nrequire inko 4.5.6";

        assert_eq!(
            Manifest::parse(&mut missing_chunks.as_bytes()),
            Err("the entry on line 2 is invalid".to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_cmd.as_bytes()),
            Err("expected line 2 to start with 'require', not 'bla'"
                .to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_version.as_bytes()),
            Err("the version on line 1 is invalid".to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_checksum.as_bytes()),
            Err("the checksum on line 1 is invalid".to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_inko_version.as_bytes()),
            Err("the version on line 1 is invalid".to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_inko_version_extra_chunk.as_bytes()),
            Err("the checksum on line 1 is invalid".to_string())
        );
        assert_eq!(
            Manifest::parse(&mut invalid_inko_version_redundant.as_bytes()),
            Err("the Inko version requirement on line 2 is invalid as a \
                requirement is already specified on line 1"
                .to_string())
        );
    }

    #[test]
    fn test_manifest_parse_valid() {
        let input = "# Ignore me
#

require inko 1.2.3
require https://gitlab.com/inko-lang/foo 1.2.3 633d02e92b2a96623c276b7d7fe09568f9f2e1ad";

        assert_eq!(
            Manifest::parse(&mut input.as_bytes()),
            Ok(Manifest {
                entries: vec![
                    Entry::Comment(" Ignore me".to_string()),
                    Entry::Comment(String::new()),
                    Entry::EmptyLine,
                    Entry::InkoVersion(Version::new(1, 2, 3)),
                    Entry::Dependency(Dependency {
                        url: Url::new("https://gitlab.com/inko-lang/foo"),
                        name: "foo".to_string(),
                        version: Version::new(1, 2, 3),
                        checksum: Checksum::new(
                            "633d02e92b2a96623c276b7d7fe09568f9f2e1ad"
                        ),
                    })
                ]
            })
        );
    }

    #[test]
    fn test_manifest_to_string() {
        let manifest = Manifest {
            entries: vec![
                Entry::Comment(" Ignore me".to_string()),
                Entry::Comment(String::new()),
                Entry::EmptyLine,
                Entry::InkoVersion(Version::new(1, 2, 3)),
                Entry::Dependency(Dependency {
                    url: Url::new("https://gitlab.com/inko-lang/foo"),
                    name: "foo".to_string(),
                    version: Version::new(1, 2, 3),
                    checksum: Checksum::new("abc"),
                }),
                Entry::Dependency(Dependency {
                    url: Url::new("https://github.com/inko-lang/bar"),
                    name: "bar".to_string(),
                    version: Version::new(4, 5, 6),
                    checksum: Checksum::new("def"),
                }),
            ],
        };

        let output = "# Ignore me
#

require inko 1.2.3
require https://gitlab.com/inko-lang/foo 1.2.3 abc
require https://github.com/inko-lang/bar 4.5.6 def
";

        assert_eq!(manifest.to_string(), output);
    }

    #[test]
    fn test_manifest_add_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let name = "test".to_string();
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url, name, version, checksum);

        assert_eq!(
            manifest.entries,
            vec![Entry::Dependency(Dependency {
                url: Url::new("test"),
                name: "test".to_string(),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            })]
        );
    }

    #[test]
    fn test_manifest_find_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let name = "test".to_string();
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), name, version, checksum);

        assert_eq!(
            manifest.find_dependency(&url),
            Some(&mut Dependency {
                url: Url::new("test"),
                name: "test".to_string(),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            })
        );
    }

    #[test]
    fn test_manifest_remove_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let name = "test".to_string();
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), name, version, checksum);
        manifest.remove_dependency(&url);

        assert!(manifest.entries.is_empty());
    }

    #[test]
    fn test_manifest_into_dependencies() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let name = "test".to_string();
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url, name, version, checksum);

        assert_eq!(
            manifest.into_dependencies(),
            vec![Dependency {
                url: Url::new("test"),
                name: "test".to_string(),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            }]
        );
    }

    #[test]
    fn test_manifest_dependencies_mut() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let name = "test".to_string();
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url, name, version, checksum);

        assert_eq!(
            manifest.dependencies_mut(),
            vec![&Dependency {
                url: Url::new("test"),
                name: "test".to_string(),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            }]
        );
    }

    #[test]
    fn test_manifest_set_inko_version() {
        let mut manifest = Manifest { entries: Vec::new() };

        assert_eq!(manifest.minimum_inko_version(), None);

        manifest.set_inko_version(Version::new(1, 2, 3));
        assert_eq!(
            manifest.minimum_inko_version(),
            Some(Version::new(1, 2, 3))
        );

        manifest.set_inko_version(Version::new(4, 5, 6));
        assert_eq!(
            manifest.minimum_inko_version(),
            Some(Version::new(4, 5, 6))
        );
        assert_eq!(manifest.entries.len(), 1);
    }
}
