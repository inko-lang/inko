mod add;
mod list;
mod remove;

use crate::error::Error;
use crate::options::print_usage;
use compiler::config::local_runtimes_directory;
use getopts::{Options, ParsingStyle};
use std::fs::create_dir_all;

const USAGE: &str = "inko runtime [OPTIONS] [COMMAND]

Manage runtimes for the available targets.

Commands:

    add     Install a new runtime
    remove  Remove an installed runtime
    list    List the available and installed runtimes

Examples:

    inko runtime add arm64-linux-gnu
    inko runtime remove arm64-linux-gnu";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    // Instead of each command having to check if the runtimes directory exists,
    // we ensure it does here.
    let runtimes = local_runtimes_directory().ok_or_else(|| {
        Error::from("failed to determine the runtimes directory".to_string())
    })?;

    if let Err(e) = create_dir_all(&runtimes) {
        return Err(Error::from(format!(
            "failed to create the runtimes directory: {}",
            e
        )));
    }

    match matches.free.first().map(|s| s.as_str()) {
        Some("add") => add::run(runtimes, &matches.free[1..]),
        Some("remove") => remove::run(runtimes, &matches.free[1..]),
        Some("list") => list::run(runtimes, &matches.free[1..]),
        Some(cmd) => {
            Err(Error::from(format!("The command {:?} is invalid", cmd)))
        }
        None => {
            print_usage(&options, USAGE);
            Ok(0)
        }
    }
}
