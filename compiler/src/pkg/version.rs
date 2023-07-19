use crate::pkg::manifest::{Dependency, Url};
use std::cmp::{Ord, Ordering};
use std::collections::HashMap;
use std::fmt;

/// The version of a dependency.
///
/// We only support versions in the format `MAJOR.MINOR.PATCH`, pre-release
/// versions are explicitly not supported.
///
/// The maximum value for each component is (2^16)-1, which should prove more
/// than sufficient for any software.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    pub fn parse(input: &str) -> Option<Self> {
        let chunks: Vec<u16> = input
            .split('.')
            .filter_map(|v| {
                // We disallow leading zeroes because why on earth would you
                // want those?
                if v.len() > 1 && v.starts_with('0') {
                    return None;
                }

                // +1 or -1 as version components makes no sense, so we reject
                // them.
                if v.starts_with('+') || v.starts_with('-') {
                    return None;
                }

                v.parse::<u16>().ok()
            })
            .collect();

        if chunks.len() == 3 {
            Some(Version::new(chunks[0], chunks[1], chunks[2]))
        } else {
            None
        }
    }

    pub fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    pub fn tag_name(&self) -> String {
        format!("v{}", self)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Returns a list of dependency URIs and their versions to use.
///
/// For duplicate dependencies, this function returns the _minimum_ version that
/// satisfies all the requirements. For example, imagine we have these
/// dependencies:
///
///     json >= 1.0
///     http >= 1.2
///     http >= 1.1
///
/// In this case the dependencies returned would be [json >= 1.0, http >= 1.2].
///
/// For more information, refer to the following links:
///
/// - https://research.swtch.com/vgo-mvs
/// - https://github.com/diku-dk/futhark/blob/master/src/Futhark/Pkg/Solve.hs
/// - https://github.com/diku-dk/smlpkg/blob/master/src/solve/solve.sml
pub fn select<'a>(
    dependencies: impl Iterator<Item = &'a Dependency>,
) -> Vec<(Url, Version)> {
    let mut versions: HashMap<(&Url, u16), &Version> = HashMap::new();

    for dep in dependencies {
        let key = (&dep.url, dep.version.major);

        match versions.get(&key) {
            Some(version) if &dep.version > version => {
                versions.insert(key, &dep.version);
            }
            None => {
                versions.insert(key, &dep.version);
            }
            _ => {}
        }
    }

    versions
        .into_iter()
        .map(|((uri, _), ver)| (uri.clone(), ver.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::manifest::Checksum;

    #[test]
    fn test_version_parse() {
        assert_eq!(Version::parse("1.2.3"), Some(Version::new(1, 2, 3)));
        assert_eq!(Version::parse("1.0.0"), Some(Version::new(1, 0, 0)));

        assert_eq!(Version::parse(""), None);
        assert_eq!(Version::parse("1"), None);
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2."), None);
        assert_eq!(Version::parse("1.2. 3"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("001.002.003"), None);
        assert_eq!(Version::parse("ff.ff.ff"), None);
        assert_eq!(Version::parse("1.2.3-alpha1"), None);
    }

    #[test]
    fn test_version_cmp() {
        assert_eq!(
            Version::new(1, 2, 0).cmp(&Version::new(1, 2, 0)),
            Ordering::Equal
        );
        assert_eq!(
            Version::new(1, 2, 1).cmp(&Version::new(1, 2, 0)),
            Ordering::Greater
        );
        assert_eq!(
            Version::new(1, 3, 0).cmp(&Version::new(1, 2, 0)),
            Ordering::Greater
        );
        assert_eq!(
            Version::new(2, 2, 0).cmp(&Version::new(1, 2, 0)),
            Ordering::Greater
        );
        assert_eq!(
            Version::new(0, 0, 0).cmp(&Version::new(1, 0, 0)),
            Ordering::Less
        );
        assert_eq!(
            Version::new(0, 0, 1).cmp(&Version::new(1, 0, 0)),
            Ordering::Less
        );
        assert_eq!(
            Version::new(0, 1, 0).cmp(&Version::new(1, 0, 0)),
            Ordering::Less
        );
        assert_eq!(
            Version::new(1, 2, 3).cmp(&Version::new(1, 2, 5)),
            Ordering::Less
        );
        assert_eq!(
            Version::new(1, 2, 5).cmp(&Version::new(1, 2, 3)),
            Ordering::Greater
        );
    }

    #[test]
    fn test_version_tag_name() {
        assert_eq!(Version::new(1, 2, 3).tag_name(), "v1.2.3".to_string());
    }

    #[test]
    fn test_select_valid() {
        let versions = select(
            [
                Dependency {
                    url: Url::new("https://gitlab.com/foo/bar"),
                    name: "bar".to_string(),
                    version: Version::new(1, 2, 3),
                    checksum: Checksum::new("a"),
                },
                Dependency {
                    url: Url::new("https://gitlab.com/foo/bar"),
                    name: "bar".to_string(),
                    version: Version::new(1, 2, 5),
                    checksum: Checksum::new("a"),
                },
                Dependency {
                    url: Url::new("https://gitlab.com/foo/json"),
                    name: "json".to_string(),
                    version: Version::new(0, 1, 2),
                    checksum: Checksum::new("a"),
                },
            ]
            .iter(),
        );

        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&(
            Url::new("https://gitlab.com/foo/bar"),
            Version::new(1, 2, 5),
        )));
        assert!(versions.contains(&(
            Url::new("https://gitlab.com/foo/json"),
            Version::new(0, 1, 2),
        )));
    }

    #[test]
    fn test_select_multiple_major_versions() {
        let versions = select(
            [
                Dependency {
                    url: Url::new("https://gitlab.com/foo/bar"),
                    name: "bar".to_string(),
                    version: Version::new(1, 2, 3),
                    checksum: Checksum::new("a"),
                },
                Dependency {
                    url: Url::new("https://gitlab.com/foo/bar"),
                    name: "bar".to_string(),
                    version: Version::new(2, 0, 0),
                    checksum: Checksum::new("a"),
                },
            ]
            .iter(),
        );

        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&(
            Url::new("https://gitlab.com/foo/bar"),
            Version::new(1, 2, 3),
        )));
        assert!(versions.contains(&(
            Url::new("https://gitlab.com/foo/bar"),
            Version::new(2, 0, 0),
        )));
    }
}
