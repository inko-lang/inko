use crate::error::Error;
use crate::git::Repository;
use crate::manifest::{Checksum, Dependency, Manifest, Url, MANIFEST_FILE};
use crate::util::{data_dir, usage};
use crate::version::Version;
use getopts::Options;

const USAGE: &str = "ipm update [OPTIONS] [PACKAGE]

Update the version requirements of one or more packages to the latest compatible
version. This command only updates the entries in the package manifest.

By default this command updates packages to their latest minor version. To
update them to the latest major version, use the -m/--major flag.

Examples:

    ipm update
    ipm update gitlab.com/inko-lang/example
    ipm update gitlab.com/inko-lang/example --major";

pub(crate) fn run(args: &[String]) -> Result<(), Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optflag("m", "major", "Update across major versions");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        usage(&options, USAGE);
        return Ok(());
    }

    let major = matches.opt_present("m");
    let mut manifest = Manifest::load(&MANIFEST_FILE)?;
    let update = if let Some(url) =
        matches.free.get(0).and_then(|uri| Url::parse(uri))
    {
        if let Some(dep) = manifest.find_dependency(&url) {
            vec![dep]
        } else {
            fail!("The package {} isn't listed in {}", url, MANIFEST_FILE);
        }
    } else {
        manifest.dependencies_mut()
    };

    for dep in update {
        let dir = data_dir()?.join(dep.url.directory_name());
        let repo = if dir.is_dir() {
            let mut repo = Repository::open(&dir)?;

            repo.fetch()?;
            repo
        } else {
            Repository::clone(&dep.url.to_string(), &dir)?
        };

        let tag_names = repo.version_tag_names();

        if tag_names.is_empty() {
            fail!("The package {} doesn't have any versions", dep.url);
        }

        let mut candidates = version_candidates(dep, tag_names, major);

        candidates.sort();

        let version = match candidates.pop() {
            Some(version) if version != dep.version => version,
            _ => continue,
        };

        println!(
            "Updating {} from version {} to version {}",
            dep.url, dep.version, version
        );

        let tag = repo.tag(&version.tag_name()).unwrap();

        dep.version = version;
        dep.checksum = Checksum::new(tag.target);
    }

    manifest.save(&MANIFEST_FILE)
}

fn version_candidates(
    dependency: &Dependency,
    tags: Vec<String>,
    major: bool,
) -> Vec<Version> {
    tags.into_iter()
        .filter_map(|v| Version::parse(&v[1..]))
        .filter(
            |v| {
                if major {
                    true
                } else {
                    v.major == dependency.version.major
                }
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_candidates() {
        let tags = vec![
            "v1.2.4".to_string(),
            "v1.3.8".to_string(),
            "v2.3.1".to_string(),
        ];
        let dep = Dependency {
            url: Url::new("gitlab.com/foo/bar"),
            version: Version::new(1, 2, 3),
            checksum: Checksum::new("abc"),
        };

        assert_eq!(
            version_candidates(&dep, tags.clone(), false),
            vec![Version::new(1, 2, 4), Version::new(1, 3, 8)]
        );

        assert_eq!(
            version_candidates(&dep, tags, true),
            vec![
                Version::new(1, 2, 4),
                Version::new(1, 3, 8),
                Version::new(2, 3, 1)
            ]
        );
    }
}
