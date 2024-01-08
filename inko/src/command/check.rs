use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::Config as CompilerConfig;
use getopts::Options;
use std::path::PathBuf;

const USAGE: &str = "Usage: inko check [OPTIONS] [FILE]

Check an entire project or a file for errors.

Examples:

    inko check                   # Check all project files
    inko check hello.inko        # Check the file hello.inko";

/// Type-checks Inko source code.
pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");
    options.optopt(
        "f",
        "format",
        "The output format to use for diagnostics",
        "FORMAT",
    );

    options.optmulti(
        "i",
        "include",
        "A directory to add to the list of source directories",
        "PATH",
    );

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = CompilerConfig::default();

    if let Some(format) = matches.opt_str("f") {
        config.set_presenter(&format)?;
    }

    for path in matches.opt_strs("i") {
        config.add_source_directory(path.into());
    }

    if config.tests.is_dir() {
        config.add_source_directory(config.tests.clone());
    }

    let mut compiler = Compiler::new(config);
    let file = matches.free.first().map(PathBuf::from);
    let result = compiler.check(file);

    compiler.print_diagnostics();

    match result {
        Ok(_) => Ok(0),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
