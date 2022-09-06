//! The main entry point for the CLI.
use crate::command::build;
use crate::command::check;
use crate::command::run;
use crate::command::test;
use crate::error::Error;
use crate::options::print_usage;
use getopts::{Options, ParsingStyle};
use std::env;

const USAGE: &str = "Usage: inko [OPTIONS] [COMMAND | FILE]

Commands:

    run      Compiles and runs FILE
    build    Compiles FILE
    test     Runs Inko unit tests

If no explicit command is given, the run command is implied. Each command takes
its own set of options.

Examples:

    inko hello.inko        # Runs the file hello.inko
    inko run hello.inko    # Same
    inko build hello.inko  # Compiles the file into a bytecode image
    inko check hello.inko  # Checks hello.inko for errors
    inko run --help        # Prints the help message for the run command";

/// Runs the default CLI command.
pub fn run() -> Result<i32, Error> {
    let args: Vec<String> = env::args().collect();
    let mut options = Options::new();

    options.parsing_style(ParsingStyle::StopAtFirstFree);
    options.optflag("h", "help", "Shows this help message");
    options.optflag("v", "version", "Prints the version number");

    let matches = options.parse(&args[1..])?;

    if matches.opt_present("h") {
        print_usage(&options, USAGE);
        return Ok(0);
    }

    if matches.opt_present("v") {
        println!("inko version {}\n", env!("CARGO_PKG_VERSION"));
        println!("AES-NI: {}", cfg!(target_feature = "aes"));
        println!("jemalloc: {}", cfg!(feature = "jemalloc"));

        return Ok(0);
    }

    match matches.free.get(0).map(|s| s.as_str()) {
        Some("run") => run::run(&matches.free[1..]),
        Some("build") => build::run(&matches.free[1..]),
        Some("check") => check::run(&matches.free[1..]),
        Some("test") => test::run(&matches.free[1..]),
        Some(_) => run::run(&matches.free),
        None => Err(Error::generic(
            "You must specify a command or input file to run".to_string(),
        )),
    }
}
