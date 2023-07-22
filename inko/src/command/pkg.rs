mod add;
mod remove;
mod sync;
mod update;

use crate::error::Error;
use crate::options::print_usage;
use getopts::{Options, ParsingStyle};

const USAGE: &str = "inko pkg [OPTIONS] [COMMAND]

Package and dependency management for Inko.

Commands:

    add     Add or update a dependency
    remove  Remove a dependency
    sync    Download and install dependencies
    update  Update all dependencies to the latest version

Examples:

    inko pkg add github.com/hello/world 1.2.3
    inko pkg sync";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    match matches.free.get(0).map(|s| s.as_str()) {
        Some("add") => add::run(&matches.free[1..]),
        Some("remove") => remove::run(&matches.free[1..]),
        Some("sync") => sync::run(&matches.free[1..]),
        Some("update") => update::run(&matches.free[1..]),
        Some(cmd) => {
            Err(Error::generic(format!("The command {:?} is invalid", cmd)))
        }
        None => {
            print_usage(&options, USAGE);
            Ok(0)
        }
    }
}
