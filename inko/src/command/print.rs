use crate::error::Error;
use crate::options::print_usage;
use compiler::config::{local_runtimes_directory, Config};
use compiler::target::Target;
use getopts::Options;

const USAGE: &str = "Usage: inko print [OPTIONS] [ARGS]

Print compiler details, such as the target, to STDOUT.

Available values:

    target   # Print the host's target triple (e.g. amd64-linux-gnu)
    runtimes # Print the paths to search for the runtime library

Examples:

    inko print target  # Print the target to STDOUT";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    match matches.free.first().map(|s| s.as_str()) {
        Some("target") => {
            println!("{}", Target::native());
            Ok(0)
        }
        Some("runtimes") => {
            if let Some(dir) = local_runtimes_directory() {
                println!("{}", dir.display());
            }

            println!("{}", Config::default().runtime.display());

            Ok(0)
        }
        Some(val) => {
            Err(Error::from(format!("'{}' isn't a valid value to print", val)))
        }
        None => Err(Error::from(
            "You must specify a type of value to print".to_string(),
        )),
    }
}
