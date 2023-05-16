use crate::error::Error;
use crate::options::print_usage;
use crate::pkg::manifest::{Manifest, Url, MANIFEST_FILE};
use getopts::Options;

const USAGE: &str = "inko pkg remove [OPTIONS] [URI]

Remove a dependency from the current project.

This command merely removes the dependency from the manifest. To also remove its
files from your project, along with any unnecessary dependencies, run
`inko pkg sync`.

Examples:

    inko pkg remove github.com/inko-lang/example";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") || matches.free.is_empty() {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let url = matches
        .free
        .get(0)
        .and_then(|uri| Url::parse(uri))
        .ok_or_else(|| "The package URL is invalid".to_string())?;

    let mut manifest = Manifest::load(&MANIFEST_FILE)?;

    manifest.remove_dependency(&url);
    manifest.save(&MANIFEST_FILE)?;
    Ok(0)
}
