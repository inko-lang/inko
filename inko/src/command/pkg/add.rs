use crate::error::Error;
use crate::options::print_usage;
use compiler::pkg::git::Repository;
use compiler::pkg::manifest::{Checksum, Manifest, Url, MANIFEST_FILE};
use compiler::pkg::util::data_dir;
use compiler::pkg::version::Version;
use getopts::Options;

const USAGE: &str = "inko pkg add [OPTIONS] [URL | \"inko\"] [VERSION]

Add a dependency to the current project.

This command merely adds the dependency to the manifest. To download it along
with its dependencies, run `inko pkg sync`.

When the first argument is \"inko\", this command sets the minimum required Inko
version to [VERSION], instead of adding a dependency.

Examples:

    inko pkg add github.com/inko-lang/example 1.2.3 # Adds a dependency
    inko pkg add inko 1.2.3                         # Sets the required version to 1.2.3";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") || matches.free.is_empty() {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    if matches.free.len() != 2 {
        return Err(Error::from(
            "You must specify a package and version to add".to_string(),
        ));
    }

    if matches.free.first().map(String::as_ref) == Some("inko") {
        let version = matches
            .free
            .get(1)
            .and_then(|version| Version::parse(version))
            .ok_or_else(|| {
                Error::from("The Inko version is invalid".to_string())
            })?;

        let mut manifest = Manifest::load(&MANIFEST_FILE)
            .map_err(|e| format!("Failed to load the manifest: {}", e))?;

        manifest.set_inko_version(version);
        manifest.save(&MANIFEST_FILE)?;

        return Ok(0);
    }

    let url =
        matches.free.first().and_then(|uri| Url::parse(uri)).ok_or_else(
            || Error::from("The package URL is invalid".to_string()),
        )?;

    let name = url.import_name();
    let version =
        matches.free.get(1).and_then(|uri| Version::parse(uri)).ok_or_else(
            || Error::from("The package version is invalid".to_string()),
        )?;
    let tag_name = version.tag_name();

    let dir = data_dir()?.join(url.directory_name());
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        (Repository::clone(&url.to_string(), &dir, &tag_name)?, false)
    };

    if fetch {
        repo.fetch()?;
    }

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
        Error::from(format!("Version {} doesn't exist", version))
    })?;

    let checksum = Checksum::new(hash);
    let mut manifest = Manifest::load(&MANIFEST_FILE)
        .map_err(|e| format!("Failed to load the manifest: {}", e))?;

    if let Some(existing) = manifest.find_dependency(&url) {
        existing.version = version;
        existing.checksum = checksum;
    } else {
        manifest.add_dependency(url, name, version, checksum);
    }

    manifest.save(&MANIFEST_FILE)?;
    Ok(0)
}
