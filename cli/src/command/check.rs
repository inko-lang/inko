//! Command for type-checking Inko source code.
use crate::compiler;
use crate::error::Error;
use crate::options::print_usage;
use getopts::Options;

const USAGE: &str = "Usage: inko check [OPTIONS]

Type-checks Inko source code, without compiling it.

Examples:

    inko check test.inko  # Type-checks test.inko and its dependencies

Output formats:

    pretty (default)
    json";

/// Checks Inko source code without compiling it.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");
    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut compiler_args = Vec::new();

    compiler_args.extend_from_slice(&matches.free);
    compiler_args.push("--check".to_string());

    if let Some(format) = matches.opt_str("f") {
        compiler_args.push("--format".to_string());
        compiler_args.push(format);
    }

    compiler::spawn(&compiler_args)
}
