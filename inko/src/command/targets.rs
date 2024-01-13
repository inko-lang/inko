use crate::error::Error;
use crate::options::print_usage;
use compiler::target::Target;
use getopts::{Options, ParsingStyle};
use std::io::{stdout, IsTerminal as _};

const USAGE: &str = "inko targets [OPTIONS]

List the supported target triples.

Examples:

    inko targets";

pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
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
        if target.is_native() {
            let line = format!("{} (native)", target);

            if is_term {
                println!("\x1b[1m{}\x1b[0m", line);
            } else {
                println!("{}", line);
            }
        } else {
            println!("{}", target);
        }
    }

    Ok(0)
}
