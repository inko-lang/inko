use crate::error::Error;
use crate::git::Repository;
use crate::manifest::{Dependency, Manifest, Url, MANIFEST_FILE};
use crate::util::{cp_r, data_dir, usage, DEP_DIR};
use crate::version::{select, Version};
use getopts::Options;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::remove_dir_all;
use std::path::{Path, PathBuf};

/// The name of the directory to copy source files from and into the ./dep
/// directory.
const SRC_DIR: &str = "src";

/// The dependant string to use for the root project.
const ROOT_DEPENDANT: &str = "Your project";

const USAGE: &str = "ipm sync [OPTIONS]

Installs all necessary dependencies into your project, and removes dependencies
no longer in use.

Examples:

    ipm sync";

#[derive(Clone)]
enum Dependant {
    Project,
    Package(Url),
}

struct Package {
    dependant: Dependant,
    repository: Repository,
    dependency: Dependency,
}

pub(crate) fn run(args: &[String]) -> Result<(), Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        usage(&options, USAGE);
        return Ok(());
    }

    println!("Updating package cache");

    let packages = download_packages()?;
    let versions = select_versions(&packages)?;
    let dep_dir = PathBuf::from(DEP_DIR);

    remove_dependencies(&dep_dir)?;
    println!("Installing");
    install_packages(packages, versions, &dep_dir)
}

fn download_packages() -> Result<Vec<Package>, Error> {
    let data_dir = data_dir()?;
    let mut manifests =
        vec![(Dependant::Project, Manifest::load(&MANIFEST_FILE)?)];
    let mut packages = Vec::new();
    let mut downloaded = HashSet::new();

    while let Some((dependant, manifest)) = manifests.pop() {
        for dep in manifest.into_dependencies() {
            let key = (dep.url.clone(), dep.version.clone());

            if downloaded.contains(&key) {
                continue;
            } else {
                downloaded.insert(key);
            }

            match download_dependency(&data_dir, dependant.clone(), dep)? {
                (package, Some(manifest)) => {
                    let dependant =
                        Dependant::Package(package.dependency.url.clone());

                    manifests.push((dependant, manifest));
                    packages.push(package);
                }
                (package, None) => packages.push(package),
            }
        }
    }

    Ok(packages)
}

fn download_dependency(
    cache_dir: &Path,
    dependant: Dependant,
    dependency: Dependency,
) -> Result<(Package, Option<Manifest>), Error> {
    let dir = cache_dir.join(dependency.url.directory_name());
    let url = dependency.url.to_string();
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        println!("  Downloading {} {}", dependency.url, dependency.version);
        (Repository::clone(&url, &dir)?, false)
    };

    let tag_name = dependency.version.tag_name();
    let tag = if let Some(tag) = repo.tag(&tag_name) {
        Some(tag)
    } else if fetch {
        println!("  Updating {}", dependency.url);

        repo.fetch()?;
        repo.tag(&tag_name)
    } else {
        None
    };

    let tag = tag.ok_or_else(|| {
        error!(
            "The version {} of package {} doesn't exist",
            dependency.version, url
        )
    })?;

    repo.checkout(&tag.target).map_err(|err| {
        error!(
            "Failed to checkout tag {} of package {}: {}",
            tag_name, url, err
        )
    })?;

    if tag.target != dependency.checksum.to_string() {
        fail!(
            "The checksum of {} version {} didn't match.

The checksum that is expected is:

    {}

The actual checksum is:

    {}

This means that either your checksum is incorrect, or the version's contents
have changed since it was last published.

If the version's contents have changed you'll need to check with the package's
maintainer to ensure this is expected.

DO NOT PROCEED BLINDLY, as you may be including unexpected or even malicious
changes.",
            url,
            dependency.version,
            dependency.checksum,
            tag.target
        );
    }

    let package = Package { dependant, repository: repo, dependency };
    let manifest_path = dir.join(MANIFEST_FILE);

    if manifest_path.is_file() {
        Ok((package, Some(Manifest::load(&manifest_path)?)))
    } else {
        Ok((package, None))
    }
}

fn select_versions(packages: &[Package]) -> Result<Vec<(Url, Version)>, Error> {
    match select(packages.iter().map(|p| &p.dependency)) {
        Ok(versions) => Ok(versions),
        Err(url) => Err(conflicting_versions_error(url, packages)),
    }
}

fn remove_dependencies(directory: &Path) -> Result<(), Error> {
    if directory.is_dir() {
        println!("Removing existing ./{}", DEP_DIR);

        remove_dir_all(&directory).map_err(|err| {
            error!("Failed to remove the existing ./{}: {}", DEP_DIR, err)
        })?;
    }

    Ok(())
}

fn install_packages(
    packages: Vec<Package>,
    versions: Vec<(Url, Version)>,
    directory: &Path,
) -> Result<(), Error> {
    let repos = packages
        .into_iter()
        .map(|pkg| (pkg.dependency.url, pkg.repository))
        .collect::<HashMap<_, _>>();

    for (url, ver) in versions {
        println!("  {} {}", url, ver);

        let repo = repos.get(&url).unwrap();
        let tag_name = ver.tag_name();
        let tag = repo.tag(&tag_name).unwrap();

        repo.checkout(&tag.target).map_err(|err| {
            error!("Failed to check out {}: {}", tag_name, err)
        })?;

        cp_r(&repo.path.join(SRC_DIR), directory)?;
    }

    Ok(())
}

fn conflicting_versions_error(url: Url, packages: &[Package]) -> Error {
    let reqs: Vec<_> = packages
        .iter()
        .filter_map(|pkg| {
            if pkg.dependency.url == url {
                let dependant = match &pkg.dependant {
                    Dependant::Project => ROOT_DEPENDANT.to_string(),
                    Dependant::Package(url) => url.to_string(),
                };

                Some(format!(
                    "{} requires:\n    >= {}, < {}.0.0",
                    dependant,
                    pkg.dependency.version,
                    pkg.dependency.version.major + 1
                ))
            } else {
                None
            }
        })
        .collect();

    error!(
        "\
The dependency graph contains conflicting major version requirements for the \
package {}.

These conflicting requirements are as follows:

  {}

To resolve these conflicts, you need to ensure all version requirements for \
package {} require the same major version.",
        url,
        reqs.join("\n\n  "),
        url
    )
}
