use crate::error::Error;
use crate::options::print_usage;
use compiler::config::Config;
use compiler::pkg::sync::sync;
use getopts::Options;

const USAGE: &str = "inko pkg sync [OPTIONS]

Install all necessary dependencies, and remove dependencies that are no longer
needed.

Examples:

    inko pkg sync";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let config = Config::default();

    sync(&config.dependencies)?;

    Ok(0)
}
