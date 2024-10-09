use crate::error::Error;
use crate::options::print_usage;
use compiler::target::Target;
use getopts::{Options, ParsingStyle};
use std::io::{stdout, IsTerminal as _};
use std::path::PathBuf;

const USAGE: &str = "inko runtime list [OPTIONS]

List the targets for which a runtime is available, and highlights those for
which a runtime is installed.

Examples:

    inko runtime list";

pub(crate) fn run(
    runtimes: PathBuf,
    arguments: &[String],
) -> Result<i32, Error> {
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;
    let is_term = stdout().is_terminal();

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    for target in Target::supported() {
        let dir = runtimes.join(target.to_string());
        let (line, bold) = if target.is_native() {
            (format!("{} (native)", target), true)
        } else if dir.is_dir() {
            (format!("{} (installed)", target), true)
        } else {
            (target.to_string(), false)
        };

        if is_term && bold {
            println!("\x1b[1m{}\x1b[0m", line);
        } else {
            println!("{}", line);
        }
    }

    Ok(0)
}
