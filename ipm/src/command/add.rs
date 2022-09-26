use crate::error::Error;
use crate::git::Repository;
use crate::manifest::{Checksum, Manifest, Url, MANIFEST_FILE};
use crate::util::{data_dir, usage};
use crate::version::Version;
use getopts::Options;

const USAGE: &str = "ipm add [OPTIONS] [URL] [VERSION]

Add a package to the manifest in the current working directory.

This command doesn't resolve any sub-dependencies or install the package into
your project, for that you need to run `ipm sync`.

Examples:

    ipm add gitlab.com/inko-lang/example 1.2.3";

pub(crate) fn run(args: &[String]) -> Result<(), Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") || matches.free.is_empty() {
        usage(&options, USAGE);
        return Ok(());
    }

    if matches.free.len() != 2 {
        fail!("You must specify a package and version to add");
    }

    let url = matches
        .free
        .get(0)
        .and_then(|uri| Url::parse(uri))
        .ok_or_else(|| error!("The package URL is invalid"))?;

    let version = matches
        .free
        .get(1)
        .and_then(|uri| Version::parse(uri))
        .ok_or_else(|| error!("The package version is invalid"))?;

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

    let hash = tag
        .map(|t| t.target)
        .ok_or_else(|| error!("Version {} doesn't exist", version))?;

    let checksum = Checksum::new(&hash);
    let mut manifest = Manifest::load(&MANIFEST_FILE)?;

    if let Some(existing) = manifest.find_dependency(&url) {
        existing.version = version;
        existing.checksum = checksum;
    } else {
        manifest.add_dependency(url, version, checksum);
    }

    manifest.save(&MANIFEST_FILE)
}
