use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::{Config, Output};
use getopts::Options;
use std::process::Command;

const USAGE: &str = "Usage: inko test [OPTIONS]

Compiles and runs unit tests

This command compiles your unit tests in ./test, then runs the resulting test
executable.

Examples:

    inko test    # Runs all unit tests in ./test";

/// Compiles and runs Inko unit tests.
pub(crate) fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Show this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let mut config = Config::default();
    let input = config.main_test_module();

    if !config.tests.is_dir() {
        return Err(Error::generic(format!(
            "The tests directory {:?} doesn't exist",
            config.tests
        )));
    }

    config.sources.push(config.tests.clone());
    config.output = Output::File("inko-tests".to_string());

    let mut compiler = Compiler::new(config);
    let result = compiler.build(Some(input));

    compiler.print_diagnostics();

    match result {
        Ok(exe) => Command::new(exe)
            .args(matches.free)
            .spawn()
            .and_then(|mut child| child.wait())
            .map_err(|err| {
                Error::generic(format!("Failed to run the tests: {}", err))
            })
            .map(|status| status.code().unwrap_or(0)),
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
