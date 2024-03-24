use crate::error::Error;
use crate::options::print_usage;
use compiler::config::Config;
use compiler::format::{Error as FormatError, Formatter, Input};
use getopts::Options;
use std::env::current_dir;
use std::path::PathBuf;

const USAGE: &str = "inko fmt [OPTIONS] [FILE]

Format source files according to the Inko style guide.

If an explicit list of files is given, only those files are updated. If no files
are given, all Inko source files in the current project are updated.

To format data passed as STDIN, specify a single \"-\" argument. In this case
the output is written to STDOUT.

To only list the files that don't use the correct formatting, use the -c/--check
option. If this option is given and there are files that don't use the correct
formatting, this command exits with exit code 1.

Examples:

    inko fmt           # Format the entire project
    inko fmt test.inko # Format a single file
    inko fmt -         # Format the data written to STDIN";

pub(crate) fn run(args: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optflag("c", "check", "Only check for formatting differences");

    let matches = options.parse(args)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let config = Config::default();
    let input = match matches.free.first().map(|v| v.as_str()) {
        Some("-") => Input::Stdin,
        Some(_) => {
            Input::Files(matches.free.iter().map(PathBuf::from).collect())
        }
        _ => Input::project(&config).map_err(Error::from)?,
    };

    let mut fmt = Formatter::new(config);

    if matches.opt_present("check") {
        match fmt.check(input) {
            Ok(paths) => {
                let cwd = current_dir().unwrap_or_else(|_| PathBuf::new());
                let status = if paths.is_empty() { 0 } else { 1 };

                for path in paths {
                    println!(
                        "{}",
                        path.strip_prefix(&cwd).unwrap_or(&path).display()
                    );
                }

                Ok(status)
            }
            Err(FormatError::Internal(e)) => Err(Error::from(e)),
            Err(FormatError::Diagnostics) => {
                fmt.print_diagnostics();
                Ok(1)
            }
        }
    } else {
        match fmt.format(input) {
            Ok(_) => Ok(0),
            Err(FormatError::Internal(e)) => Err(Error::from(e)),
            Err(FormatError::Diagnostics) => {
                fmt.print_diagnostics();
                Ok(1)
            }
        }
    }
}
