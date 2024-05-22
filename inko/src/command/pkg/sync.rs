use crate::error::Error;
use crate::options::print_usage;
use crate::pkg::git::Repository;
use crate::pkg::util::{cp_r, data_dir};
use compiler::config::Config;
use compiler::pkg::manifest::{Dependency, Manifest, Url, MANIFEST_FILE};
use compiler::pkg::version::{select, Version};
use getopts::Options;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::{copy, remove_dir_all};
use std::path::Path;

/// The name of the directory to copy source files from and into the ./dep
/// directory.
const SRC_DIR: &str = "src";

const USAGE: &str = "inko pkg sync [OPTIONS]

Install all necessary dependencies, and remove dependencies that are no longer
needed.

Examples:

    inko pkg sync";

struct Package {
    repository: Repository,
    dependency: Dependency,
}

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let config = Config::default();
    let packages = download_packages()?;
    let versions = select(packages.iter().map(|p| &p.dependency));

    remove_dependencies(&config.dependencies)?;
    install_packages(packages, versions, &config.dependencies)?;
    Ok(0)
}

fn download_packages() -> Result<Vec<Package>, Error> {
    let data_dir = data_dir()?;
    let mut manifests = vec![Manifest::load(&MANIFEST_FILE)?];
    let mut packages = Vec::new();
    let mut downloaded = HashSet::new();

    while let Some(manifest) = manifests.pop() {
        for dep in manifest.into_dependencies() {
            let key = (dep.url.clone(), dep.version.clone());

            if downloaded.contains(&key) {
                continue;
            } else {
                downloaded.insert(key);
            }

            match download_dependency(&data_dir, dep)? {
                (package, Some(manifest)) => {
                    manifests.push(manifest);
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
    dependency: Dependency,
) -> Result<(Package, Option<Manifest>), Error> {
    let dir = cache_dir.join(dependency.url.directory_name());
    let url = dependency.url.to_string();
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        println!("Downloading {} v{}", dependency.url, dependency.version);
        (Repository::clone(&url, &dir)?, false)
    };

    let tag_name = dependency.version.tag_name();
    let tag = if let Some(tag) = repo.tag(&tag_name) {
        Some(tag)
    } else if fetch {
        println!("Updating {}", dependency.url);
        repo.fetch()?;
        repo.tag(&tag_name)
    } else {
        None
    };

    let tag = tag.ok_or_else(|| {
        format!(
            "The version {} of package {} doesn't exist",
            dependency.version, url
        )
    })?;

    repo.checkout(&tag.target).map_err(|err| {
        format!(
            "Failed to checkout tag {} of package {}: {}",
            tag_name, url, err
        )
    })?;

    if tag.target != dependency.checksum.to_string() {
        format!(
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
            url, dependency.version, dependency.checksum, tag.target
        );
    }

    let package = Package { repository: repo, dependency };
    let manifest_path = dir.join(MANIFEST_FILE);

    if manifest_path.is_file() {
        Ok((package, Some(Manifest::load(&manifest_path)?)))
    } else {
        Ok((package, None))
    }
}

fn remove_dependencies(directory: &Path) -> Result<(), String> {
    if directory.is_dir() {
        remove_dir_all(directory).map_err(|err| {
            format!("Failed to remove {}: {}", directory.display(), err)
        })?;
    }

    Ok(())
}

fn install_packages(
    packages: Vec<Package>,
    versions: Vec<(Url, Version)>,
    directory: &Path,
) -> Result<(), String> {
    let repos = packages
        .into_iter()
        .map(|pkg| (pkg.dependency.url, pkg.repository))
        .collect::<HashMap<_, _>>();

    for (url, ver) in versions {
        let repo = repos.get(&url).unwrap();
        let tag_name = ver.tag_name();
        let tag = repo.tag(&tag_name).unwrap();

        repo.checkout(&tag.target).map_err(|err| {
            format!("Failed to check out {}: {}", tag_name, err)
        })?;

        let base_dir = directory
            .join(url.directory_name())
            .join(&format!("v{}", ver.major));

        let manifest_src = repo.path.join(MANIFEST_FILE);

        cp_r(&repo.path.join(SRC_DIR), &base_dir.join(SRC_DIR))?;

        if manifest_src.is_file() {
            copy(&manifest_src, &base_dir.join(MANIFEST_FILE))
                .map_err(|e| format!("Failed to copy inko.pkg: {}", e))?;
        }
    }

    Ok(())
}
