use crate::error::Error;
use crate::manifest::{Manifest, Url, MANIFEST_FILE};
use crate::util::usage;
use getopts::Options;

const USAGE: &str = "ipm remove [OPTIONS] [URI]

Remove a dependency from the manifest.

This command doesn't remove the dependency from your project, for that you need
to run `ipm sync`.

Examples:

    ipm remove github.com/inko-lang/example";

pub(crate) fn run(args: &[String]) -> Result<(), Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") || matches.free.is_empty() {
        usage(&options, USAGE);
        return Ok(());
    }

    let url = matches
        .free
        .get(0)
        .and_then(|uri| Url::parse(uri))
        .ok_or_else(|| error!("The package URL is invalid"))?;

    let mut manifest = Manifest::load(&MANIFEST_FILE)?;

    manifest.remove_dependency(&url);
    manifest.save(&MANIFEST_FILE)
}
