use crate::error::Error;
use crate::options::print_usage;
use crate::pkg::git::Repository;
use crate::pkg::manifest::{Checksum, Manifest, Url, MANIFEST_FILE};
use crate::pkg::util::data_dir;
use crate::pkg::version::Version;
use getopts::Options;

const USAGE: &str = "inko pkg add [OPTIONS] [URL] [VERSION]

Add a dependency to the current project.

This command merely adds the dependency to the manifest. To download it along
with its dependencies, run `inko pkg sync`.

Examples:

    inko pkg add github.com/inko-lang/example 1.2.3";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") || matches.free.is_empty() {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    if matches.free.len() != 2 {
        return Err(Error::generic(
            "You must specify a package and version to add".to_string(),
        ));
    }

    let url = matches.free.get(0).and_then(|uri| Url::parse(uri)).ok_or_else(
        || Error::generic("The package URL is invalid".to_string()),
    )?;

    let version =
        matches.free.get(1).and_then(|uri| Version::parse(uri)).ok_or_else(
            || Error::generic("The package version is invalid".to_string()),
        )?;

    let dir = data_dir()?.join(url.directory_name());
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        (Repository::clone(&url.to_string(), &dir)?, false)
    };

    if fetch {
        repo.fetch()?;
    }

    let tag_name = version.tag_name();
    let tag = if let Some(tag) = repo.tag(&tag_name) {
        Some(tag)
    } else if fetch {
        println!("Updating {}", url);
        repo.fetch()?;
        repo.tag(&tag_name)
    } else {
        None
    };

    let hash = tag.map(|t| t.target).ok_or_else(|| {
        Error::generic(format!("Version {} doesn't exist", version))
    })?;

    let checksum = Checksum::new(hash);
    let mut manifest = Manifest::load(&MANIFEST_FILE)?;

    if let Some(existing) = manifest.find_dependency(&url) {
        existing.version = version;
        existing.checksum = checksum;
    } else {
        manifest.add_dependency(url, version, checksum);
    }

    manifest.save(&MANIFEST_FILE)?;
    Ok(0)
}
