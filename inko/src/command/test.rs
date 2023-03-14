//! Command for running unit tests.
use crate::error::Error;
use crate::options::print_usage;
use compiler::compiler::{CompileError, Compiler};
use compiler::config::Config as CompilerConfig;
use getopts::Options;
use vm::config::Config;

const USAGE: &str = "Usage: inko test [OPTIONS]

Compiles and runs unit tests

This command adds your tests directory to the module load path, and runs the
`main.inko` file that resides in this tests directory.

Examples:

    inko test    # Runs all unit tests in ./test";

/// Compiles and runs Inko unit tests.
pub fn run(arguments: &[String]) -> Result<i32, Error> {
    let mut options = Options::new();

    options.optflag("h", "help", "Shows this help message");

    let matches = options.parse(arguments)?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    let arguments = &matches.free;
    let mut config = CompilerConfig::default();
    let input = config.main_test_module();
    let tests = config.tests.clone();

    if !tests.is_dir() {
        return Err(Error::generic(format!(
            "The tests directory {:?} doesn't exist",
            tests
        )));
    }

    config.sources.add(tests);

    let mut compiler = Compiler::new(config);
    let result = compiler.compile_to_memory(Some(input));

    compiler.print_diagnostics();

    match result {
        Ok(bytes) => {
            // TODO: compile to object file and run
            todo!("make 'inko test' work again")
            // let config = Config::from_env();
            // let image = Image::load_bytes(config, bytes).map_err(|e| {
            //     Error::generic(format!("Failed to parse bytecode image: {}", e))
            // })?;
            //
            // Machine::boot(image, arguments).map_err(Error::generic)
        }
        Err(CompileError::Invalid) => Ok(1),
        Err(CompileError::Internal(msg)) => Err(Error::generic(msg)),
    }
}
