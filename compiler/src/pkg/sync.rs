use crate::pkg::git::Repository;
use crate::pkg::manifest::{Dependency, Manifest, Url, MANIFEST_FILE};
use crate::pkg::util::{cp_r, data_dir};
use crate::pkg::version::{select, Version};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::{
    copy, create_dir_all, read, read_to_string, remove_dir_all, write,
};
use std::io;
use std::path::Path;

/// The name of the directory to copy source files from and into the ./dep
/// directory.
const SRC_DIR: &str = "src";

struct Package {
    repository: Repository,
    dependency: Dependency,
}

pub fn sync_if_needed(directory: &Path) -> Result<(), String> {
    if manifest_hash_changed(directory)? {
        sync(directory)
    } else {
        Ok(())
    }
}

pub fn sync(directory: &Path) -> Result<(), String> {
    let packages = download_packages()?;
    let versions = select(packages.iter().map(|p| &p.dependency));

    remove_dependencies(directory)?;
    install_packages(packages, versions, directory)?;
    save_manifest_hash(directory)?;
    Ok(())
}

fn download_packages() -> Result<Vec<Package>, String> {
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
) -> Result<(Package, Option<Manifest>), String> {
    let dir = cache_dir.join(dependency.url.directory_name());
    let url = dependency.url.to_string();
    let tag_name = dependency.version.tag_name();
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        println!("Downloading {} v{}", dependency.url, dependency.version);
        (Repository::clone(&url, &dir, &tag_name)?, false)
    };

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

        repo.checkout(&tag.target)
            .map_err(|e| format!("Failed to check out {}: {}", tag_name, e))?;

        let base_dir = directory
            .join(url.directory_name())
            .join(&format!("v{}", ver.major));

        let manifest_src = repo.path.join(MANIFEST_FILE);

        cp_r(&repo.path.join(SRC_DIR), &base_dir.join(SRC_DIR))?;

        if manifest_src.is_file() {
            let to = base_dir.join(MANIFEST_FILE);

            copy(&manifest_src, &to).map_err(|e| {
                format!(
                    "Failed to copy {} to {}: {}",
                    manifest_src.display(),
                    to.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn manifest_hash() -> Result<Option<String>, String> {
    let data = match read(MANIFEST_FILE) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!("Failed to read {}: {}", MANIFEST_FILE, e));
        }
    };

    Ok(Some(blake3::hash(&data).to_string()))
}

fn save_manifest_hash(directory: &Path) -> Result<(), String> {
    let Some(hash) = manifest_hash()? else {
        return Ok(());
    };

    let hash_file = directory.join("hash");

    create_dir_all(directory).map_err(|err| {
        format!("Failed to create {}: {}", directory.display(), err)
    })?;

    write(&hash_file, hash).map_err(|err| {
        format!("Failed to update {}: {}", hash_file.display(), err)
    })?;

    Ok(())
}

fn manifest_hash_changed(directory: &Path) -> Result<bool, String> {
    let hash = manifest_hash()?;
    let hash_file = directory.join("hash");

    let saved_hash = match read_to_string(&hash_file) {
        Ok(data) => Some(data),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(format!(
                "Failed to read {}: {}",
                hash_file.display(),
                err
            ));
        }
    };

    Ok(hash != saved_hash)
}
