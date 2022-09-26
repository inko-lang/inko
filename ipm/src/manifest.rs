use crate::error::Error;
use crate::version::Version;
use blake2::{digest::consts::U16, Blake2b, Digest};
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

pub(crate) const MANIFEST_FILE: &str = "inko.pkg";

/// The URL of a package.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub(crate) struct Url {
    pub(crate) value: String,
}

impl Url {
    pub(crate) fn parse(input: &str) -> Option<Self> {
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

    pub(crate) fn new<S: Into<String>>(value: S) -> Self {
        Self { value: value.into() }
    }

    pub(crate) fn directory_name(&self) -> String {
        // We don't need ultra long hashes, as all we care about is being able
        // to generate a directory name from a URL _without_ it colliding with
        // literally everything.
        let mut hasher: Blake2b<U16> = Blake2b::new();

        hasher.update(&self.value);
        format!("{:x}", hasher.finalize())
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

/// A Git (SHA1) checksum.
#[derive(Eq, PartialEq, Debug, Clone)]
pub(crate) struct Checksum {
    pub(crate) value: String,
}

impl Checksum {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        if input.len() != 40 {
            return None;
        }

        Some(Checksum::new(input))
    }

    pub(crate) fn new<S: Into<String>>(value: S) -> Self {
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
pub(crate) struct Dependency {
    pub(crate) url: Url,
    pub(crate) version: Version,
    pub(crate) checksum: Checksum,
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "require {} {} {}", self.url, self.version, self.checksum)
    }
}

#[derive(Eq, PartialEq, Debug)]
pub(crate) enum Entry {
    Comment(String),
    Dependency(Dependency),
    EmptyLine,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Entry::Comment(comment) => write!(f, "#{}", comment),
            Entry::EmptyLine => Ok(()),
            Entry::Dependency(dep) => dep.fmt(f),
        }
    }
}

/// A dependency manifest parsed from a `inko.pkg` file.
#[derive(Eq, PartialEq, Debug)]
pub(crate) struct Manifest {
    pub(crate) entries: Vec<Entry>,
}

impl Manifest {
    pub(crate) fn load<P: AsRef<Path>>(path: &P) -> Result<Self, Error> {
        let path = path.as_ref();

        File::open(path)
            .map_err(|e| error!("Failed to read {}: {}", path.display(), e))
            .and_then(|mut file| Self::parse(&mut file))
    }

    pub(crate) fn parse<R: Read>(stream: &mut R) -> Result<Self, Error> {
        let reader = BufReader::new(stream);
        let mut manifest = Self { entries: Vec::new() };

        for (index, line) in reader.lines().enumerate() {
            let lnum = index + 1;
            let line = line.map_err(|err| {
                error!("Failed to read lines from the manifest: {}", err)
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

            if chunks.len() != 4 {
                fail!("The entry on line {} is invalid", lnum);
            }

            // Currently this is the only action we support.
            if chunks[0] != "require" {
                fail!(
                    "Expected line {} to start with 'require', not '{}'",
                    lnum,
                    chunks[0]
                );
            }

            let url = Url::parse(chunks[1])
                .ok_or_else(|| error!("The URI on line {} is invalid", lnum))?;
            let version = Version::parse(chunks[2]).ok_or_else(|| {
                error!("The version on line {} is invalid", lnum)
            })?;
            let checksum = Checksum::parse(chunks[3]).ok_or_else(|| {
                error!("The checksum on line {} is invalid", lnum)
            })?;

            manifest.entries.push(Entry::Dependency(Dependency {
                url,
                version,
                checksum,
            }));
        }

        Ok(manifest)
    }

    pub(crate) fn add_dependency(
        &mut self,
        url: Url,
        version: Version,
        checksum: Checksum,
    ) {
        self.entries.push(Entry::Dependency(Dependency {
            url,
            version,
            checksum,
        }));
    }

    pub(crate) fn find_dependency(
        &mut self,
        url: &Url,
    ) -> Option<&mut Dependency> {
        self.entries.iter_mut().find_map(|entry| match entry {
            Entry::Dependency(dep) if &dep.url == url => Some(dep),
            _ => None,
        })
    }

    pub(crate) fn remove_dependency(&mut self, url: &Url) {
        self.entries.retain(
            |val| !matches!(val, Entry::Dependency(dep) if &dep.url == url),
        )
    }

    pub(crate) fn dependencies_mut(&mut self) -> Vec<&mut Dependency> {
        self.entries
            .iter_mut()
            .filter_map(|entry| match entry {
                Entry::Dependency(dep) => Some(dep),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn into_dependencies(self) -> Vec<Dependency> {
        self.entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Dependency(dep) => Some(dep),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn save<P: AsRef<Path>>(&self, path: &P) -> Result<(), Error> {
        let path = path.as_ref();

        File::create(path)
            .and_then(|mut file| file.write_all(self.to_string().as_bytes()))
            .map_err(|e| error!("Failed to update {}: {}", path.display(), e))
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
            "4efb5ddfa8b68f5e1885fc8b75838f43".to_string()
        );
        assert_eq!(
            Url::new("http://gitlab.com/foo/bar").directory_name(),
            "2334066f1e6f5fea14ebf3fb71f714ca".to_string()
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

        assert_eq!(
            Manifest::parse(&mut missing_chunks.as_bytes()),
            Err(Error::new("The entry on line 2 is invalid".to_string()))
        );
        assert_eq!(
            Manifest::parse(&mut invalid_cmd.as_bytes()),
            Err(Error::new(
                "Expected line 2 to start with 'require', not 'bla'"
                    .to_string()
            ))
        );
        assert_eq!(
            Manifest::parse(&mut invalid_version.as_bytes()),
            Err(Error::new("The version on line 1 is invalid".to_string()))
        );
        assert_eq!(
            Manifest::parse(&mut invalid_checksum.as_bytes()),
            Err(Error::new("The checksum on line 1 is invalid".to_string()))
        );
    }

    #[test]
    fn test_manifest_parse_valid() {
        let input = "# Ignore me
#

require https://gitlab.com/inko-lang/foo 1.2.3 633d02e92b2a96623c276b7d7fe09568f9f2e1ad";

        assert_eq!(
            Manifest::parse(&mut input.as_bytes()),
            Ok(Manifest {
                entries: vec![
                    Entry::Comment(" Ignore me".to_string()),
                    Entry::Comment(String::new()),
                    Entry::EmptyLine,
                    Entry::Dependency(Dependency {
                        url: Url::new("https://gitlab.com/inko-lang/foo"),
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
                Entry::Dependency(Dependency {
                    url: Url::new("https://gitlab.com/inko-lang/foo"),
                    version: Version::new(1, 2, 3),
                    checksum: Checksum::new("abc"),
                }),
                Entry::Dependency(Dependency {
                    url: Url::new("https://github.com/inko-lang/bar"),
                    version: Version::new(4, 5, 6),
                    checksum: Checksum::new("def"),
                }),
            ],
        };

        let output = "# Ignore me
#

require https://gitlab.com/inko-lang/foo 1.2.3 abc
require https://github.com/inko-lang/bar 4.5.6 def
";

        assert_eq!(manifest.to_string(), output);
    }

    #[test]
    fn test_manifest_add_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url, version, checksum);

        assert_eq!(
            manifest.entries,
            vec![Entry::Dependency(Dependency {
                url: Url::new("test"),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            })]
        );
    }

    #[test]
    fn test_manifest_find_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), version, checksum);

        assert_eq!(
            manifest.find_dependency(&url),
            Some(&mut Dependency {
                url: Url::new("test"),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            })
        );
    }

    #[test]
    fn test_manifest_remove_dependency() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), version, checksum);
        manifest.remove_dependency(&url);

        assert!(manifest.entries.is_empty());
    }

    #[test]
    fn test_manifest_into_dependencies() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), version, checksum);

        assert_eq!(
            manifest.into_dependencies(),
            vec![Dependency {
                url: Url::new("test"),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            }]
        );
    }

    #[test]
    fn test_manifest_dependencies_mut() {
        let mut manifest = Manifest { entries: Vec::new() };
        let url = Url::new("test");
        let version = Version::new(1, 2, 3);
        let checksum = Checksum::new("abc");

        manifest.add_dependency(url.clone(), version, checksum);

        assert_eq!(
            manifest.dependencies_mut(),
            vec![&Dependency {
                url: Url::new("test"),
                version: Version::new(1, 2, 3),
                checksum: Checksum::new("abc")
            }]
        );
    }
}
