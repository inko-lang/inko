use crate::error::Error;
use crate::options::print_usage;
use compiler::target::Target;
use getopts::{Options, ParsingStyle};
use std::fs::remove_dir_all;
use std::path::PathBuf;

const USAGE: &str = "inko runtime remove [OPTIONS] TARGET

Remove an existing runtime for a given target.

Examples:

    inko runtime remove arm64-linux-gnu";

pub(crate) fn run(
    runtimes: PathBuf,
    arguments: &[String],
) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let target =
        matches.free.first().and_then(|v| Target::parse(v)).ok_or_else(
            || Error::from("a valid target triple is required".to_string()),
        )?;

    let dir = runtimes.join(target.to_string());

    if !dir.is_dir() {
        return Err(Error::from(format!(
            "no runtime for the target '{}' is installed",
            target
        )));
    }

    remove_dir_all(&dir)
        .map_err(|e| {
            Error::from(format!("failed to remove {}: {}", dir.display(), e))
        })
        .map(|_| 0)
}
