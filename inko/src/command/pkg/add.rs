use crate::error::Error;
use crate::options::print_usage;
use compiler::pkg::git::Repository;
use compiler::pkg::manifest::{Checksum, MANIFEST_FILE, Manifest, Url};
use compiler::pkg::util::{Status, data_dir};
use compiler::pkg::version::Version;
use getopts::Options;

const USAGE: &str = "inko pkg add [OPTIONS] [URI | \"inko\"] [VERSION]

Add a dependency to the current project.

This command merely adds the dependency to the manifest. To download it along
with its dependencies, run `inko pkg sync`.

When the first argument is \"inko\", this command sets the minimum required Inko
version to [VERSION], instead of adding a dependency.

Examples:

    inko pkg add github.com/inko-lang/example 1.2.3 # Add a dependency using a specific version
    inko pkg add github.com/inko-lang/example       # Add a dependency using the latest version
    inko pkg add inko 1.2.3                         # Set the required Inko version to 1.2.3
    inko pkg add inko                               # Set the required Inko version to the current version";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    match matches.free.first().map(String::as_str) {
        Some("inko") if matches.free.len() <= 2 => {
            let ver = matches
                .free
                .get(1)
                .map(String::as_str)
                .unwrap_or(env!("CARGO_PKG_VERSION"));

            add_inko_version(ver)
        }
        Some("inko") => {
            print_usage(&options, USAGE);
            Ok(1)
        }
        Some(uri) if matches.free.len() <= 2 => {
            add_package_version(uri, matches.free.get(1).map(String::as_str))
        }
        _ => {
            print_usage(&options, USAGE);
            Ok(1)
        }
    }
}

fn add_inko_version(version: &str) -> Result<i32, Error> {
    let version = Version::parse(version)
        .ok_or_else(|| Error::from("the version is invalid".to_string()))?;
    let mut manifest = Manifest::load(&MANIFEST_FILE)
        .map_err(|e| format!("failed to load the manifest: {}", e))?;

    manifest.set_inko_version(version);
    manifest.save(&MANIFEST_FILE)?;
    Ok(0)
}

fn add_package_version(uri: &str, version: Option<&str>) -> Result<i32, Error> {
    let url = Url::parse(uri)
        .ok_or_else(|| Error::from("the package URL is invalid".to_string()))?;
    let (data, _data_lock) = data_dir()?;
    let dir = data.join(url.directory_name());
    let mut status = Status::new();
    let (mut repo, fetch) = if dir.is_dir() {
        (Repository::open(&dir)?, true)
    } else {
        status.set(&url, "downloading");
        (Repository::full_clone(&url.to_string(), &dir)?, false)
    };

    let version = match version {
        // inko pkg add foo 1.2.3
        Some(v) => Version::parse(v).ok_or_else(|| {
            Error::from(format!("the version '{}' is invalid", v))
        })?,
        // inko pkg add foo
        _ => {
            if fetch {
                status.set(&url, "updating");
                repo.fetch()?;
            }

            repo.latest_version().ok_or_else(|| {
                Error::from("the package doesn't have any versions".to_string())
            })?
        }
    };

    let name = url.import_name();
    let tag_name = version.tag_name();
    let tag = if let Some(tag) = repo.tag(&tag_name) {
        Some(tag)
    } else if fetch {
        status.set(&url, "updating");
        repo.fetch()?;
        repo.tag(&tag_name)
    } else {
        None
    };
    let tag = tag.ok_or_else(|| {
        Error::from(format!("version {} doesn't exist", version))
    })?;
    let checksum = Checksum::new(tag.target);
    let mut manifest = Manifest::load(&MANIFEST_FILE)
        .map_err(|e| format!("failed to load the manifest: {}", e))?;

    if let Some(existing) = manifest.find_dependency(&url) {
        existing.version = version;
        existing.checksum = checksum;
    } else {
        manifest.add_dependency(url, name, version, checksum);
    }

    manifest.save(&MANIFEST_FILE)?;
    Ok(0)
}
